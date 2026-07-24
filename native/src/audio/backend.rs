use crate::audio::state::SharedState;
use cpal::traits::DeviceTrait;
use ringbuf::traits::{Consumer, Observer};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use symphonia::core::sample::SampleFormat as SymphoniaSampleFormat;

const TARGET_OUTPUT_BUFFER_MS: u32 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_current: bool,
}

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

pub(crate) fn build_stream<S, C>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut consumer: C,
    state: Arc<SharedState>,
    channels: usize,
) -> Result<cpal::Stream, Box<dyn std::error::Error>>
where
    S: cpal::SizedSample + Send + 'static,
    C: Consumer<Item = S> + Observer<Item = S> + Send + 'static,
{
    let stream = device.build_output_stream(
        config,
        move |data: &mut [S], info: &cpal::OutputCallbackInfo| {
            drain_discarded_buffer(&mut consumer, &state);

            if should_wait_for_buffer(consumer.occupied_len(), channels, &state) {
                data.fill(S::EQUILIBRIUM);
                return;
            }

            if state.is_paused.load(Ordering::Relaxed) {
                data.fill(S::EQUILIBRIUM);
                return;
            }

            let mut samples_read = 0usize;
            for sample in data.iter_mut() {
                if let Some(s) = consumer.try_pop() {
                    *sample = s;
                    samples_read += 1;
                } else {
                    *sample = S::EQUILIBRIUM;
                }
            }

            if samples_read > 0 {
                let frames_read = (samples_read / channels) as u64;
                let buffer_start_frame = state
                    .current_frame
                    .fetch_add(frames_read, Ordering::Relaxed);
                state.update_playback_clock_from_output(
                    buffer_start_frame,
                    buffer_start_frame.saturating_add(frames_read),
                    output_latency(info),
                );
            } else if state.decoder_done.load(Ordering::Relaxed)
                && !state.is_finished.swap(true, Ordering::SeqCst)
            {
                state.finish_notify.notify_waiters();
            }
        },
        |_err| {},
        None,
    )?;
    Ok(stream)
}

pub(crate) fn build_stream_converted<In, Out, C>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut consumer: C,
    state: Arc<SharedState>,
    channels: usize,
) -> Result<cpal::Stream, Box<dyn std::error::Error>>
where
    In: Copy + Send + 'static,
    Out: cpal::SizedSample + cpal::FromSample<In> + Send + 'static,
    C: Consumer<Item = In> + Observer<Item = In> + Send + 'static,
{
    let stream = device.build_output_stream(
        config,
        move |data: &mut [Out], info: &cpal::OutputCallbackInfo| {
            drain_discarded_buffer(&mut consumer, &state);

            if should_wait_for_buffer(consumer.occupied_len(), channels, &state) {
                data.fill(Out::EQUILIBRIUM);
                return;
            }

            if state.is_paused.load(Ordering::Relaxed) {
                data.fill(Out::EQUILIBRIUM);
                return;
            }

            let mut samples_read = 0usize;
            for sample in data.iter_mut() {
                if let Some(s) = consumer.try_pop() {
                    *sample = Out::from_sample(s);
                    samples_read += 1;
                } else {
                    *sample = Out::EQUILIBRIUM;
                }
            }

            if samples_read > 0 {
                let frames_read = (samples_read / channels) as u64;
                let buffer_start_frame = state
                    .current_frame
                    .fetch_add(frames_read, Ordering::Relaxed);
                state.update_playback_clock_from_output(
                    buffer_start_frame,
                    buffer_start_frame.saturating_add(frames_read),
                    output_latency(info),
                );
            } else if state.decoder_done.load(Ordering::Relaxed)
                && !state.is_finished.swap(true, Ordering::SeqCst)
            {
                state.finish_notify.notify_waiters();
            }
        },
        |_err| {},
        None,
    )?;
    Ok(stream)
}

fn output_latency(info: &cpal::OutputCallbackInfo) -> Duration {
    let timestamp = info.timestamp();
    timestamp
        .playback
        .duration_since(&timestamp.callback)
        .unwrap_or(Duration::ZERO)
}

fn drain_discarded_buffer<S, C>(consumer: &mut C, state: &SharedState)
where
    C: Consumer<Item = S>,
{
    if state.discard_buffer.load(Ordering::SeqCst) {
        state.is_discarding_buffer.store(true, Ordering::SeqCst);
        state.discard_buffer.store(false, Ordering::SeqCst);
        while consumer.try_pop().is_some() {}
        state.is_discarding_buffer.store(false, Ordering::SeqCst);
    }
}

fn should_wait_for_buffer(buffered_samples: usize, channels: usize, state: &SharedState) -> bool {
    if state.has_seek_request.load(Ordering::Relaxed) {
        return true;
    }

    let sr = state.sample_rate.load(Ordering::Relaxed) as usize;
    let decoder_done = state.decoder_done.load(Ordering::Relaxed);
    let min_samples_to_resume = sr * channels;

    if state.waiting_for_seek.load(Ordering::Relaxed) {
        if buffered_samples >= min_samples_to_resume || decoder_done {
            state.waiting_for_seek.store(false, Ordering::Relaxed);
            return false;
        }

        return true;
    }

    if buffered_samples == 0 && !decoder_done {
        state.waiting_for_seek.store(true, Ordering::Relaxed);
        return true;
    }

    false
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

fn target_output_buffer_size(
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

fn device_alsa_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|desc| desc.name().to_string())
        .unwrap_or_else(|_| "Unknown Device".to_string())
}

pub(crate) fn device_id(device: &cpal::Device) -> String {
    let alsa_name = device_alsa_name(device);

    #[cfg(target_os = "linux")]
    {
        use alsa::device_name::HintIter;
        use std::ffi::CStr;

        if let Ok(iter) = HintIter::new(None, unsafe {
            CStr::from_bytes_with_nul_unchecked(b"pcm\0")
        }) {
            for hint in iter {
                if let Some(hint_name) = hint.name {
                    if hint_name == alsa_name {
                        return hint_name;
                    }
                }
            }
        }
    }

    alsa_name
}

pub(crate) fn device_display_name(device: &cpal::Device) -> String {
    let alsa_name = device_alsa_name(device);

    #[cfg(target_os = "linux")]
    {
        use alsa::device_name::HintIter;
        use std::ffi::CStr;

        if let Ok(iter) = HintIter::new(None, unsafe {
            CStr::from_bytes_with_nul_unchecked(b"pcm\0")
        }) {
            for hint in iter {
                if let Some(hint_name) = hint.name {
                    if hint_name == alsa_name {
                        if let Some(desc) = hint.desc {
                            if let Some(first_line) = desc.lines().next() {
                                let friendly_name = first_line.trim().to_string();
                                if !friendly_name.is_empty() {
                                    return friendly_name;
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    alsa_name
}

#[cfg(target_os = "linux")]
pub(crate) fn linux_plughw_locator(id: &str) -> Option<String> {
    if let Some(stripped) = id.strip_prefix("alsa:hw:") {
        return Some(resolve_linux_hw_device_id(&format!("hw:{}", stripped)));
    }

    if id.starts_with("hw:") || id.starts_with("plughw:") {
        return Some(resolve_linux_hw_device_id(id));
    }

    None
}

pub(crate) fn should_list_linux_output_device(
    id: &str,
    _default_id: Option<&str>,
    _current_id: Option<&str>,
) -> bool {
    let normalized = id.trim().to_ascii_lowercase();
    if is_linux_real_hardware_output_id(&normalized) {
        return true;
    }

    false
}

#[cfg(target_os = "linux")]
pub(crate) fn append_linux_alsa_hint_output_devices(
    devices: &mut Vec<OutputDeviceInfo>,
    default_id: Option<&str>,
    current_id: Option<&str>,
) {
    use alsa::device_name::HintIter;
    use std::ffi::CStr;

    let Ok(iter) = HintIter::new(None, unsafe {
        CStr::from_bytes_with_nul_unchecked(b"pcm\0")
    }) else {
        return;
    };

    for hint in iter {
        let Some(id) = hint.name else {
            continue;
        };

        if !should_list_linux_output_device(&id, default_id, current_id) {
            continue;
        }

        if devices.iter().any(|device| device.id == id) {
            continue;
        }

        let name = hint
            .desc
            .and_then(|desc| desc.lines().next().map(|line| line.trim().to_string()))
            .filter(|desc| !desc.is_empty())
            .unwrap_or_else(|| id.clone());

        devices.push(OutputDeviceInfo {
            is_default: default_id == Some(id.as_str()),
            is_current: current_id == Some(id.as_str()),
            id,
            name,
        });
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn append_linux_proc_asound_output_devices(
    devices: &mut Vec<OutputDeviceInfo>,
    default_id: Option<&str>,
    current_id: Option<&str>,
) {
    append_linux_proc_asound_output_devices_at(
        devices,
        default_id,
        current_id,
        std::path::Path::new("/proc/asound"),
    );
}

#[cfg(target_os = "linux")]
pub(crate) fn append_linux_proc_asound_output_devices_at(
    devices: &mut Vec<OutputDeviceInfo>,
    default_id: Option<&str>,
    current_id: Option<&str>,
    asound_root: &std::path::Path,
) {
    let cards = parse_proc_asound_cards(&asound_root.join("cards"));
    let Ok(pcm) = std::fs::read_to_string(asound_root.join("pcm")) else {
        return;
    };

    for line in pcm.lines() {
        let Some((card_index, device_index, pcm_name)) = parse_proc_asound_playback_pcm_line(line)
        else {
            continue;
        };
        let Some(card) = cards.iter().find(|card| card.index == card_index) else {
            continue;
        };

        let id = format!("hw:CARD={},DEV={}", card.index, device_index);
        let name = format!("{} - {}", card.display_name(), pcm_name);

        if let Some(device) = devices.iter_mut().find(|device| device.id == id) {
            if device.name == device.id || device.name.trim().is_empty() {
                device.name = name;
            }
            device.is_default |= default_id == Some(id.as_str());
            device.is_current |= current_id == Some(id.as_str());
            continue;
        }

        devices.push(OutputDeviceInfo {
            is_default: default_id == Some(id.as_str()),
            is_current: current_id == Some(id.as_str()),
            id,
            name,
        });
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn collapse_linux_duplicate_output_devices(devices: &mut Vec<OutputDeviceInfo>) {
    let hw_ids: std::collections::HashSet<String> = devices
        .iter()
        .filter_map(|device| linux_hw_twin_id(&device.id))
        .collect();

    devices.retain(|device| {
        if device.id.trim().to_ascii_lowercase().starts_with("plughw:") {
            return !hw_ids.contains(&normalize_linux_hw_id(&device.id));
        }
        true
    });
}

#[cfg(target_os = "linux")]
fn linux_hw_twin_id(id: &str) -> Option<String> {
    let normalized = id.trim();
    if !normalized.to_ascii_lowercase().starts_with("hw:") {
        return None;
    }
    Some(normalize_linux_hw_id(normalized))
}

#[cfg(target_os = "linux")]
fn normalize_linux_hw_id(id: &str) -> String {
    id.trim()
        .strip_prefix("plughw:")
        .or_else(|| id.trim().strip_prefix("hw:"))
        .unwrap_or_else(|| id.trim())
        .to_ascii_lowercase()
}

pub(crate) fn is_linux_real_hardware_output_id(id: &str) -> bool {
    let normalized = id.trim().to_ascii_lowercase();
    normalized.starts_with("hw:")
        || normalized.starts_with("plughw:")
        || normalized.starts_with("alsa:hw:")
}

#[cfg(target_os = "linux")]
fn resolve_linux_hw_device_id(id: &str) -> String {
    let Some(card_token) = extract_alsa_card_token(id) else {
        return id.to_string();
    };
    if card_token.parse::<u32>().is_ok() {
        return id.to_string();
    }

    let Some(card_index) = alsa_card_index_from_device_id(id) else {
        return id.to_string();
    };

    let prefix = if id.trim().starts_with("plughw:") {
        "plughw"
    } else {
        "hw"
    };
    let device_index = extract_alsa_device_token(id).unwrap_or(0);
    format!("{prefix}:CARD={card_index},DEV={device_index}")
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcAsoundCard {
    index: u32,
    id: String,
    name: String,
    long_name: String,
}

#[cfg(target_os = "linux")]
impl ProcAsoundCard {
    fn display_name(&self) -> String {
        let long_name = self.long_name.trim();
        if !long_name.is_empty() {
            return long_name
                .split_once(" at ")
                .map(|(name, _)| name.trim())
                .unwrap_or(long_name)
                .to_string();
        }

        self.name.clone()
    }
}

#[cfg(target_os = "linux")]
fn parse_proc_asound_cards(path: &std::path::Path) -> Vec<ProcAsoundCard> {
    let Ok(cards) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut result = Vec::new();
    let mut lines = cards.lines();
    while let Some(header) = lines.next() {
        let Some(card) = parse_proc_asound_card_header(header) else {
            continue;
        };
        let long_name = lines.next().unwrap_or_default().trim().to_string();
        result.push(ProcAsoundCard { long_name, ..card });
    }
    result
}

#[cfg(target_os = "linux")]
fn parse_proc_asound_card_header(line: &str) -> Option<ProcAsoundCard> {
    let (index_text, rest) = line.trim_start().split_once(' ')?;
    let index = index_text.parse::<u32>().ok()?;
    let bracket_start = rest.find('[')?;
    let bracket_end = rest[bracket_start + 1..].find(']')? + bracket_start + 1;
    let id = rest[bracket_start + 1..bracket_end].trim().to_string();
    let (_, name_part) = rest[bracket_end + 1..].split_once(':')?;
    let name = name_part
        .split_once('-')
        .map(|(_, value)| value.trim())
        .unwrap_or_else(|| name_part.trim())
        .to_string();

    if id.is_empty() {
        return None;
    }

    Some(ProcAsoundCard {
        index,
        id,
        name,
        long_name: String::new(),
    })
}

#[cfg(target_os = "linux")]
fn parse_proc_asound_playback_pcm_line(line: &str) -> Option<(u32, u32, String)> {
    if !line.contains("playback") {
        return None;
    }

    let (address, rest) = line.split_once(':')?;
    let (card_text, device_text) = address.trim().split_once('-')?;
    let card_index = card_text.parse::<u32>().ok()?;
    let device_index = device_text.parse::<u32>().ok()?;
    let pcm_name = rest
        .split(':')
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("PCM")
        .to_string();

    Some((card_index, device_index, pcm_name))
}

#[cfg(target_os = "linux")]
pub(crate) fn alsa_reservation_name_for_device_id(id: &str) -> Option<String> {
    alsa_card_index_from_device_id(id).map(|card_index| format!("Audio{card_index}"))
}

#[cfg(target_os = "linux")]
pub(crate) fn alsa_card_index_from_device_id(id: &str) -> Option<u32> {
    alsa_card_index_from_device_id_at(id, std::path::Path::new("/proc/asound"))
}

#[cfg(target_os = "linux")]
pub(crate) fn alsa_card_index_from_device_id_at(
    id: &str,
    asound_root: &std::path::Path,
) -> Option<u32> {
    let card_token = extract_alsa_card_token(id)?;

    if let Ok(card_index) = card_token.parse::<u32>() {
        return Some(card_index);
    }

    let entries = std::fs::read_dir(asound_root).ok()?;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let Some(card_index) = file_name
            .strip_prefix("card")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let card_id_path = entry.path().join("id");
        let Ok(card_id) = std::fs::read_to_string(card_id_path) else {
            continue;
        };

        if card_id.trim() == card_token {
            return Some(card_index);
        }
    }

    None
}

#[cfg(target_os = "linux")]
fn extract_alsa_card_token(id: &str) -> Option<String> {
    let id = id
        .trim()
        .strip_prefix("alsa:")
        .unwrap_or_else(|| id.trim())
        .trim();
    let id = id
        .strip_prefix("hw:")
        .or_else(|| id.strip_prefix("plughw:"))?
        .trim();

    if let Some(rest) = id.strip_prefix("CARD=") {
        return Some(rest.split(',').next()?.trim().to_string()).filter(|token| !token.is_empty());
    }

    Some(id.split(',').next()?.trim().to_string()).filter(|token| !token.is_empty())
}

#[cfg(target_os = "linux")]
fn extract_alsa_device_token(id: &str) -> Option<u32> {
    let id = id
        .trim()
        .strip_prefix("alsa:")
        .unwrap_or_else(|| id.trim())
        .trim();
    let id = id
        .strip_prefix("hw:")
        .or_else(|| id.strip_prefix("plughw:"))?
        .trim();

    if let Some((_, rest)) = id.split_once("DEV=") {
        return rest
            .split(',')
            .next()
            .and_then(|value| value.trim().parse::<u32>().ok());
    }

    id.split(',')
        .nth(1)
        .and_then(|value| value.trim().parse::<u32>().ok())
}

pub(crate) fn disambiguate_output_device_names(devices: &mut [OutputDeviceInfo]) {
    use std::collections::HashMap;
    let mut name_counts = HashMap::new();
    for d in devices.iter() {
        *name_counts.entry(d.name.clone()).or_insert(0) += 1;
    }

    for d in devices.iter_mut() {
        if *name_counts.get(&d.name).unwrap() > 1 {
            d.name = format!("{} [{}]", d.name, d.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::state::{NO_TRIM_FRAME, SharedState};
    use ringbuf::HeapRb;
    use ringbuf::traits::{Observer, Producer, Split};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use tokio::sync::Notify;

    fn create_state(sample_rate: u32) -> SharedState {
        SharedState {
            is_paused: AtomicBool::new(false),
            current_frame: AtomicU64::new(0),
            playback_clock: Mutex::new(crate::audio::state::PlaybackClock::new()),
            trim_until_frame: AtomicU64::new(NO_TRIM_FRAME),
            sample_rate: AtomicU32::new(sample_rate),
            has_seek_request: AtomicBool::new(false),
            seek_request: Mutex::new(None),
            is_terminating: AtomicBool::new(false),
            discard_buffer: AtomicBool::new(false),
            is_discarding_buffer: AtomicBool::new(false),
            decoder_done: AtomicBool::new(false),
            is_finished: AtomicBool::new(false),
            finish_notify: Notify::new(),
            buffered_frames: AtomicU64::new(0),
            waiting_for_seek: AtomicBool::new(false),
        }
    }

    #[test]
    fn preferred_output_formats_prioritizes_source_format() {
        let formats = preferred_output_formats(Some(24), Some(SymphoniaSampleFormat::F32));

        assert_eq!(formats[0], cpal::SampleFormat::F32);
        assert_eq!(formats[1], cpal::SampleFormat::F64);
        assert!(formats.contains(&cpal::SampleFormat::I24));
    }

    #[test]
    fn preferred_output_formats_keeps_fallbacks_unique() {
        let formats = preferred_output_formats(Some(16), Some(SymphoniaSampleFormat::S16));

        let mut unique_formats = Vec::new();
        for format in &formats {
            if !unique_formats.contains(format) {
                unique_formats.push(*format);
            }
        }

        let unique_count = unique_formats.len();
        assert_eq!(unique_count, formats.len());
        assert_eq!(formats[0], cpal::SampleFormat::I16);
        assert!(formats.contains(&cpal::SampleFormat::F32));
    }

    #[test]
    fn target_output_buffer_size_uses_low_latency_fixed_frames_when_supported() {
        assert_eq!(
            target_output_buffer_size(
                &cpal::SupportedBufferSize::Range {
                    min: 128,
                    max: 4096
                },
                48_000
            ),
            cpal::BufferSize::Fixed(960)
        );
        assert_eq!(
            target_output_buffer_size(
                &cpal::SupportedBufferSize::Range {
                    min: 2048,
                    max: 4096
                },
                48_000
            ),
            cpal::BufferSize::Fixed(2048)
        );
    }

    #[test]
    fn old_samples_without_discard_would_release_seek_buffering() {
        let sample_rate = 48_000;
        let channels = 2;
        let state = create_state(sample_rate);
        let rb = HeapRb::<i16>::new(sample_rate as usize * channels * 2);
        let (mut producer, consumer) = rb.split();
        let old_samples = vec![1_i16; sample_rate as usize * channels + 1024];
        assert_eq!(producer.push_slice(&old_samples), old_samples.len());

        state.waiting_for_seek.store(true, Ordering::SeqCst);

        assert!(
            !should_wait_for_buffer(consumer.occupied_len(), channels, &state),
            "bug condition: old pre-seek samples satisfy the resume threshold"
        );
        assert!(!state.waiting_for_seek.load(Ordering::SeqCst));
    }

    #[test]
    fn pending_seek_request_keeps_old_samples_from_resuming_output() {
        let sample_rate = 48_000;
        let channels = 2;
        let state = create_state(sample_rate);
        let rb = HeapRb::<i16>::new(sample_rate as usize * channels * 2);
        let (mut producer, consumer) = rb.split();
        let old_samples = vec![1_i16; sample_rate as usize * channels + 1024];
        assert_eq!(producer.push_slice(&old_samples), old_samples.len());

        state.waiting_for_seek.store(true, Ordering::SeqCst);
        state.has_seek_request.store(true, Ordering::SeqCst);

        assert!(should_wait_for_buffer(
            consumer.occupied_len(),
            channels,
            &state
        ));
        assert!(state.waiting_for_seek.load(Ordering::SeqCst));
    }

    #[test]
    fn pending_discard_drains_old_samples_before_seek_resume_check() {
        let sample_rate = 48_000;
        let channels = 2;
        let state = create_state(sample_rate);
        let rb = HeapRb::<i16>::new(sample_rate as usize * channels * 2);
        let (mut producer, mut consumer) = rb.split();
        let old_samples = vec![1_i16; sample_rate as usize * channels + 1024];
        assert_eq!(producer.push_slice(&old_samples), old_samples.len());

        state.discard_buffer.store(true, Ordering::SeqCst);
        state.waiting_for_seek.store(true, Ordering::SeqCst);

        drain_discarded_buffer(&mut consumer, &state);

        assert_eq!(consumer.occupied_len(), 0);
        assert!(should_wait_for_buffer(
            consumer.occupied_len(),
            channels,
            &state
        ));
        assert!(state.waiting_for_seek.load(Ordering::SeqCst));
        assert!(!state.discard_buffer.load(Ordering::SeqCst));
    }

    /// 反衬测试：seek 完成、解码器已把 *正确目标位置* 样本写入 ringbuf 后，若再像旧
    /// `handle_seek_if_needed` 那样二次置位 `discard_buffer`，回调进入
    /// `drain_discarded_buffer` 会把这些目标样本一并排空——ringbuf 回到空，进度停在
    /// seek 锚点而听不到声音，正是“进度与实际播放位置不一致”。修复去掉了二次拉高，
    /// 此处复现旧行为以证明其危害。
    #[test]
    fn legacy_post_seek_discard_would_flush_correct_target_samples() {
        let sample_rate = 48_000;
        let channels = 2;
        let state = create_state(sample_rate);
        let rb = HeapRb::<i16>::new(sample_rate as usize * channels * 2);
        let (mut producer, mut consumer) = rb.split();

        // seek 锚点已设（见 decoder.rs），随后解码写入正确目标位置样本。
        state.waiting_for_seek.store(true, Ordering::SeqCst);
        let target_samples = vec![2_i16; sample_rate as usize * channels + 1024];
        assert_eq!(producer.push_slice(&target_samples), target_samples.len());
        assert!(consumer.occupied_len() >= sample_rate as usize * channels);

        // 旧行为：decode 侧在 format.seek 之后再置 discard_buffer=true。
        state.discard_buffer.store(true, Ordering::SeqCst);

        // 回调线程进来处理 discard → 目标样本被全部排空。
        drain_discarded_buffer(&mut consumer, &state);

        assert_eq!(
            consumer.occupied_len(),
            0,
            "BUG 重现：旧行为二次置 discard 把正确的目标位置样本全部排空，\
             导致播放无声而 current_frame 仍向前推进 → 进度与播放脱节"
        );

        // 验证 ~1 秒目标样本被清空后，回调会重新进入等待/再缓冲状态。
        assert!(should_wait_for_buffer(
            consumer.occupied_len(),
            channels,
            &state
        ));
    }

    /// 互补正向测试：修复后 seek 完成不再二次拉高 discard，目标样本完好保留，
    /// 缓冲达成阈值即可正常恢复播放，current_frame 与播放位置一致前进。
    #[test]
    fn post_seek_no_second_discard_preserves_target_samples() {
        let sample_rate = 48_000;
        let channels = 2;
        let state = create_state(sample_rate);
        let rb = HeapRb::<i16>::new(sample_rate as usize * channels * 2);
        let (mut producer, consumer) = rb.split();

        state.waiting_for_seek.store(true, Ordering::SeqCst);
        let target_samples = vec![2_i16; sample_rate as usize * channels + 1024];
        assert_eq!(producer.push_slice(&target_samples), target_samples.len());

        assert_eq!(consumer.occupied_len(), target_samples.len());
        // 目标样本同样会被回调当作正常音频消费、计入 current_frame，与实际播放一致。
        assert!(
            !should_wait_for_buffer(consumer.occupied_len(), channels, &state),
            "修复后目标样本驱动播放恢复正常，不再误清空"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_plughw_locator_rewrites_hw_device_ids() {
        assert_eq!(
            linux_plughw_locator("alsa:hw:CARD=Device,DEV=0").as_deref(),
            Some("hw:CARD=Device,DEV=0")
        );
        assert_eq!(
            linux_plughw_locator("hw:CARD=3,DEV=1").as_deref(),
            Some("hw:CARD=3,DEV=1")
        );
        assert_eq!(
            linux_plughw_locator("plughw:CARD=3,DEV=0").as_deref(),
            Some("plughw:CARD=3,DEV=0")
        );
        assert_eq!(linux_plughw_locator("default"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_output_device_filter_skips_virtual_defaults_but_keeps_real_devices() {
        assert!(should_list_linux_output_device(
            "hw:CARD=0,DEV=0",
            Some("default"),
            Some("default")
        ));
        assert!(!should_list_linux_output_device(
            "default",
            Some("default"),
            Some("default")
        ));
        assert!(!should_list_linux_output_device(
            "pulse",
            Some("hw:CARD=0,DEV=0"),
            Some("default")
        ));
        assert!(should_list_linux_output_device(
            "hw:CARD=UsbDac,DEV=0",
            Some("pipewire"),
            Some("pipewire")
        ));
        assert!(should_list_linux_output_device(
            "plughw:CARD=UsbDac,DEV=0",
            Some("pipewire"),
            Some("pipewire")
        ));
    }

    #[test]
    fn bit_perfect_format_requires_exact_source_format() {
        assert_eq!(
            exact_output_format(Some(24), Some(SymphoniaSampleFormat::S24)),
            Some(cpal::SampleFormat::I24)
        );
        assert_eq!(
            bit_perfect_output_formats(Some(24), Some(SymphoniaSampleFormat::S24)),
            vec![cpal::SampleFormat::I24, cpal::SampleFormat::I32]
        );
        assert_eq!(
            bit_perfect_output_formats(Some(24), None),
            vec![cpal::SampleFormat::I24, cpal::SampleFormat::I32]
        );
        assert_eq!(
            exact_output_format(Some(16), Some(SymphoniaSampleFormat::S16)),
            Some(cpal::SampleFormat::I16)
        );
        assert_eq!(exact_output_format(None, None), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn alsa_card_index_resolves_named_card_id_for_reservation() {
        let root = std::env::temp_dir().join(format!("ncm-asound-test-{}", std::process::id()));
        let card_dir = root.join("card2");
        std::fs::create_dir_all(&card_dir).unwrap();
        std::fs::write(card_dir.join("id"), "UsbDac\n").unwrap();

        assert_eq!(
            alsa_card_index_from_device_id_at("hw:CARD=UsbDac,DEV=0", &root),
            Some(2)
        );
        assert_eq!(
            alsa_card_index_from_device_id_at("plughw:CARD=UsbDac,DEV=0", &root),
            Some(2)
        );
        assert_eq!(alsa_card_index_from_device_id_at("hw:3,0", &root), Some(3));

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn named_hw_device_ids_are_resolved_to_numeric_cpal_ids() {
        let root =
            std::env::temp_dir().join(format!("ncm-asound-resolve-test-{}", std::process::id()));
        let card_dir = root.join("card4");
        std::fs::create_dir_all(&card_dir).unwrap();
        std::fs::write(card_dir.join("id"), "UsbDac\n").unwrap();

        assert_eq!(
            alsa_card_index_from_device_id_at("hw:CARD=UsbDac,DEV=2", &root),
            Some(4)
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn proc_asound_fallback_lists_real_playback_pcm_devices() {
        let root =
            std::env::temp_dir().join(format!("ncm-asound-proc-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("cards"),
            " 0 [PCH            ]: HDA-Intel - HDA Intel PCH\n                      HDA Intel PCH at 0x622d1b8000 irq 233\n 1 [UsbDac         ]: USB-Audio - Reference USB DAC\n                      Reference USB DAC at usb-0000:00:14.0-3, high speed\n",
        )
        .unwrap();
        std::fs::write(
            root.join("pcm"),
            "00-00: ALC287 Analog : ALC287 Analog : playback 1 : capture 1\n00-03: HDMI 0 : HDMI 0 : playback 1\n01-00: USB Audio : USB Audio : playback 1 : capture 1\n",
        )
        .unwrap();

        let mut devices = Vec::new();
        append_linux_proc_asound_output_devices_at(
            &mut devices,
            None,
            Some("hw:CARD=1,DEV=0"),
            &root,
        );

        assert!(devices.iter().any(|device| {
            device.id == "hw:CARD=1,DEV=0"
                && device.name == "Reference USB DAC - USB Audio"
                && device.is_current
        }));
        assert!(devices.iter().any(|device| device.id == "hw:CARD=0,DEV=3"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn proc_asound_fallback_updates_existing_hw_names_and_collapses_plughw_twins() {
        let root =
            std::env::temp_dir().join(format!("ncm-asound-collapse-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("cards"),
            " 1 [UsbDac         ]: USB-Audio - Reference USB DAC\n                      Reference USB DAC at usb-0000:00:14.0-3, high speed\n",
        )
        .unwrap();
        std::fs::write(
            root.join("pcm"),
            "01-00: USB Audio : USB Audio : playback 1\n",
        )
        .unwrap();

        let mut devices = vec![
            OutputDeviceInfo {
                id: "hw:CARD=1,DEV=0".to_string(),
                name: "hw:CARD=1,DEV=0".to_string(),
                is_default: false,
                is_current: false,
            },
            OutputDeviceInfo {
                id: "plughw:CARD=1,DEV=0".to_string(),
                name: "plughw:CARD=1,DEV=0".to_string(),
                is_default: false,
                is_current: false,
            },
        ];

        append_linux_proc_asound_output_devices_at(&mut devices, None, None, &root);
        collapse_linux_duplicate_output_devices(&mut devices);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "hw:CARD=1,DEV=0");
        assert_eq!(devices[0].name, "Reference USB DAC - USB Audio");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_output_device_names_are_disambiguated_with_ids() {
        let mut devices = vec![
            OutputDeviceInfo {
                id: "hw:CARD=0,DEV=0".to_string(),
                name: "USB DAC".to_string(),
                is_default: false,
                is_current: false,
            },
            OutputDeviceInfo {
                id: "hw:CARD=1,DEV=0".to_string(),
                name: "USB DAC".to_string(),
                is_default: false,
                is_current: true,
            },
            OutputDeviceInfo {
                id: "default".to_string(),
                name: "System Default".to_string(),
                is_default: true,
                is_current: false,
            },
        ];

        disambiguate_output_device_names(&mut devices);

        assert_eq!(devices[0].name, "USB DAC [hw:CARD=0,DEV=0]");
        assert_eq!(devices[1].name, "USB DAC [hw:CARD=1,DEV=0]");
        assert_eq!(devices[2].name, "System Default");
    }
}
