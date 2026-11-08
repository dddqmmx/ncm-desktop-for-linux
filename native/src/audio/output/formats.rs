//! 输出格式协商：普通播放挑"最贴近音源且设备支持"的格式，BitPerfect 只接受
//! 与音源逐位一致的格式（宁可拒绝也不让 ALSA 在背后重采样）。
//!
//! 纯计算 + `supported_output_configs()` 查询，不涉及播放时序。

use cpal::traits::DeviceTrait;
use symphonia::core::sample::SampleFormat as SymphoniaSampleFormat;

const TARGET_OUTPUT_BUFFER_MS: u32 = 20;

pub(crate) fn is_supported_output_format(fmt: cpal::SampleFormat) -> bool {
    matches!(
        fmt,
        cpal::SampleFormat::I8
            | cpal::SampleFormat::U8
            | cpal::SampleFormat::I16
            | cpal::SampleFormat::U16
            | cpal::SampleFormat::I24
            | cpal::SampleFormat::U24
            | cpal::SampleFormat::I32
            | cpal::SampleFormat::U32
            | cpal::SampleFormat::F32
            | cpal::SampleFormat::F64
    )
}

pub(crate) fn preferred_output_formats(
    bits_per_sample: Option<u32>,
    source_sample_format: Option<SymphoniaSampleFormat>,
) -> Vec<cpal::SampleFormat> {
    let mut prefer = Vec::new();

    if let Some(fmt) = source_sample_format {
        match fmt {
            SymphoniaSampleFormat::F32 => {
                prefer.push(cpal::SampleFormat::F32);
                prefer.push(cpal::SampleFormat::F64);
            }
            SymphoniaSampleFormat::F64 => prefer.push(cpal::SampleFormat::F64),
            SymphoniaSampleFormat::S16 => prefer.push(cpal::SampleFormat::I16),
            SymphoniaSampleFormat::S24 => prefer.push(cpal::SampleFormat::I24),
            SymphoniaSampleFormat::S32 => prefer.push(cpal::SampleFormat::I32),
            SymphoniaSampleFormat::S8 => prefer.push(cpal::SampleFormat::I8),
            SymphoniaSampleFormat::U16 => prefer.push(cpal::SampleFormat::U16),
            SymphoniaSampleFormat::U24 => prefer.push(cpal::SampleFormat::U24),
            SymphoniaSampleFormat::U32 => prefer.push(cpal::SampleFormat::U32),
            SymphoniaSampleFormat::U8 => prefer.push(cpal::SampleFormat::U8),
        }
    }

    match bits_per_sample {
        Some(32) => {
            if !prefer.contains(&cpal::SampleFormat::F32) {
                prefer.push(cpal::SampleFormat::F32);
            }
            if !prefer.contains(&cpal::SampleFormat::I32) {
                prefer.push(cpal::SampleFormat::I32);
            }
        }
        Some(24) => {
            if !prefer.contains(&cpal::SampleFormat::I24) {
                prefer.push(cpal::SampleFormat::I24);
            }
            if !prefer.contains(&cpal::SampleFormat::F32) {
                prefer.push(cpal::SampleFormat::F32);
            }
            if !prefer.contains(&cpal::SampleFormat::I32) {
                prefer.push(cpal::SampleFormat::I32);
            }
        }
        _ => {}
    }

    let fallbacks = [
        cpal::SampleFormat::F32,
        cpal::SampleFormat::I16,
        cpal::SampleFormat::I32,
    ];
    for f in fallbacks {
        if !prefer.contains(&f) {
            prefer.push(f);
        }
    }

    prefer
}

pub(crate) fn find_best_config(
    device: &cpal::Device,
    target_sr: u32,
    channels: u16,
    bits_per_sample: Option<u32>,
    source_sample_format: Option<SymphoniaSampleFormat>,
) -> Result<(cpal::StreamConfig, cpal::SampleFormat), Box<dyn std::error::Error>> {
    println!(
        "[audio] target request: sample_rate={}Hz, channels={}",
        target_sr, channels
    );

    let mut candidates = Vec::new();
    for c in device.supported_output_configs()? {
        if c.channels() == channels
            && target_sr >= c.min_sample_rate()
            && target_sr <= c.max_sample_rate()
        {
            candidates.push(c);
        }
    }

    let prefer = preferred_output_formats(bits_per_sample, source_sample_format);
    for fmt in prefer.iter() {
        if let Some(c) = candidates.iter().find(|c| c.sample_format() == *fmt) {
            let config = stream_config_with_target_buffer(c, target_sr);
            return Ok((config, *fmt));
        }
    }

    if let Some(c) = candidates
        .iter()
        .find(|c| is_supported_output_format(c.sample_format()))
    {
        let config = stream_config_with_target_buffer(c, target_sr);
        return Ok((config, c.sample_format()));
    }

    Err("Hardware doesn't support file's sample-rate/channels in compatible sample format".into())
}

#[cfg(test)]
pub(crate) fn exact_output_format(
    bits_per_sample: Option<u32>,
    source_sample_format: Option<SymphoniaSampleFormat>,
) -> Option<cpal::SampleFormat> {
    bit_perfect_output_formats(bits_per_sample, source_sample_format)
        .into_iter()
        .next()
}

pub(crate) fn bit_perfect_output_formats(
    bits_per_sample: Option<u32>,
    source_sample_format: Option<SymphoniaSampleFormat>,
) -> Vec<cpal::SampleFormat> {
    if let Some(fmt) = source_sample_format {
        return match fmt {
            SymphoniaSampleFormat::F32 => vec![cpal::SampleFormat::F32],
            SymphoniaSampleFormat::F64 => vec![cpal::SampleFormat::F64],
            SymphoniaSampleFormat::S16 => vec![cpal::SampleFormat::I16],
            SymphoniaSampleFormat::S24 => vec![cpal::SampleFormat::I24, cpal::SampleFormat::I32],
            SymphoniaSampleFormat::S32 => vec![cpal::SampleFormat::I32],
            SymphoniaSampleFormat::S8 => vec![cpal::SampleFormat::I8],
            SymphoniaSampleFormat::U16 => vec![cpal::SampleFormat::U16],
            SymphoniaSampleFormat::U24 => vec![cpal::SampleFormat::U24, cpal::SampleFormat::U32],
            SymphoniaSampleFormat::U32 => vec![cpal::SampleFormat::U32],
            SymphoniaSampleFormat::U8 => vec![cpal::SampleFormat::U8],
        };
    }

    match bits_per_sample {
        Some(8) => vec![cpal::SampleFormat::I8],
        Some(16) => vec![cpal::SampleFormat::I16],
        Some(24) => vec![cpal::SampleFormat::I24, cpal::SampleFormat::I32],
        Some(32) => vec![cpal::SampleFormat::I32],
        _ => Vec::new(),
    }
}

pub(crate) fn find_bit_perfect_config(
    device: &cpal::Device,
    target_sr: u32,
    channels: u16,
    bits_per_sample: Option<u32>,
    source_sample_format: Option<SymphoniaSampleFormat>,
) -> Result<(cpal::StreamConfig, cpal::SampleFormat), Box<dyn std::error::Error>> {
    let required_formats = bit_perfect_output_formats(bits_per_sample, source_sample_format);
    if required_formats.is_empty() {
        return Err("当前无法满足BitPerfect条件拒绝播放：无法确定音源样本格式".into());
    }

    let mut supported_channels = std::collections::BTreeSet::new();
    let mut supported_formats = std::collections::BTreeSet::new();
    let mut sample_rate_supported_for_channel = false;
    let mut format_supported_for_channel_rate = false;

    for c in device.supported_output_configs()? {
        supported_channels.insert(c.channels());
        supported_formats.insert(format!("{:?}", c.sample_format()));

        if c.channels() == channels
            && target_sr >= c.min_sample_rate()
            && target_sr <= c.max_sample_rate()
        {
            sample_rate_supported_for_channel = true;
        }

        if c.channels() == channels
            && required_formats.contains(&c.sample_format())
            && target_sr >= c.min_sample_rate()
            && target_sr <= c.max_sample_rate()
        {
            format_supported_for_channel_rate = true;
        }

        if c.channels() == channels
            && required_formats.contains(&c.sample_format())
            && target_sr >= c.min_sample_rate()
            && target_sr <= c.max_sample_rate()
        {
            let config = stream_config_with_target_buffer(&c, target_sr);
            return Ok((config, c.sample_format()));
        }
    }

    let mut reasons = Vec::new();
    if !supported_channels.contains(&channels) {
        let supported = supported_channels
            .iter()
            .map(|value| format!("{value}ch"))
            .collect::<Vec<_>>()
            .join("、");
        reasons.push(format!(
            "声道数不支持：音源为 {}ch，设备支持 {}",
            channels,
            if supported.is_empty() {
                "未知".to_string()
            } else {
                supported
            }
        ));
    } else if !sample_rate_supported_for_channel {
        reasons.push(format!(
            "采样率不支持：音源为 {}Hz，设备没有同时支持 {}ch 的该采样率",
            target_sr, channels
        ));
    }

    if !format_supported_for_channel_rate {
        let supported = supported_formats.into_iter().collect::<Vec<_>>().join("、");
        let required = required_formats
            .iter()
            .map(|format| format!("{format:?}"))
            .collect::<Vec<_>>()
            .join(" 或 ");
        reasons.push(format!(
            "位深/样本格式不支持：音源需要 {}，设备支持 {}",
            required,
            if supported.is_empty() {
                "未知".to_string()
            } else {
                supported
            }
        ));
    }

    Err(format!(
        "当前无法满足BitPerfect条件拒绝播放：{}\n音源格式：{}Hz / {}ch / {:?}",
        if reasons.is_empty() {
            "设备不支持该音源的精确输出格式".to_string()
        } else {
            reasons.join("；")
        },
        target_sr,
        channels,
        required_formats
            .iter()
            .map(|format| format!("{format:?}"))
            .collect::<Vec<_>>()
            .join(" 或 ")
    )
    .into())
}

fn stream_config_with_target_buffer(
    config: &cpal::SupportedStreamConfigRange,
    sample_rate: u32,
) -> cpal::StreamConfig {
    let mut stream_config: cpal::StreamConfig = config.with_sample_rate(sample_rate).into();
    stream_config.buffer_size = target_output_buffer_size(config.buffer_size(), sample_rate);
    stream_config
}

pub(crate) fn target_output_buffer_size(
    supported: &cpal::SupportedBufferSize,
    sample_rate: u32,
) -> cpal::BufferSize {
    let target_frames = (sample_rate / (1_000 / TARGET_OUTPUT_BUFFER_MS)).max(1);
    match *supported {
        cpal::SupportedBufferSize::Range { min, max } => {
            cpal::BufferSize::Fixed(target_frames.clamp(min, max))
        }
        cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Default,
    }
}
