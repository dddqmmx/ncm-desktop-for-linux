use cpal::traits::DeviceTrait;
use ringbuf::traits::{Consumer, Observer};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use symphonia::core::sample::SampleFormat as SymphoniaSampleFormat;
use crate::audio::state::SharedState;

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
        move |data: &mut [S], _: &cpal::OutputCallbackInfo| {
            if state.discard_buffer.swap(false, Ordering::SeqCst) {
                while consumer.try_pop().is_some() {}
            }

            let buffered_samples = consumer.occupied_len();
            let sr = state.sample_rate.load(Ordering::Relaxed) as usize;
            let decoder_done = state.decoder_done.load(Ordering::Relaxed);
            let min_samples_to_resume = sr * channels;

            let mut is_buffering = state.waiting_for_seek.load(Ordering::Relaxed);

            if is_buffering {
                if buffered_samples >= min_samples_to_resume || decoder_done {
                    state.waiting_for_seek.store(false, Ordering::Relaxed);
                    println!("[Audio] 缓冲结束，当前 Buffer: {} 采样", buffered_samples);
                } else {
                    data.fill(S::EQUILIBRIUM);
                    return;
                }
            }

            if !is_buffering && buffered_samples == 0 && !decoder_done {
                state.waiting_for_seek.store(true, Ordering::Relaxed);
                is_buffering = true;
            }

            if is_buffering {
                if buffered_samples >= min_samples_to_resume || decoder_done {
                    state.waiting_for_seek.store(false, Ordering::Relaxed);
                } else {
                    data.fill(S::EQUILIBRIUM);
                    return;
                }
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
                state
                    .current_frame
                    .fetch_add((samples_read / channels) as u64, Ordering::Relaxed);
            } else if decoder_done && !state.is_finished.swap(true, Ordering::SeqCst) {
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
        move |data: &mut [Out], _: &cpal::OutputCallbackInfo| {
            if state.discard_buffer.swap(false, Ordering::SeqCst) {
                while consumer.try_pop().is_some() {}
            }

            let buffered_samples = consumer.occupied_len();
            let sr = state.sample_rate.load(Ordering::Relaxed) as usize;
            let decoder_done = state.decoder_done.load(Ordering::Relaxed);
            let min_samples_to_resume = sr * channels;

            let mut is_buffering = state.waiting_for_seek.load(Ordering::Relaxed);
            if is_buffering {
                if buffered_samples >= min_samples_to_resume || decoder_done {
                    state.waiting_for_seek.store(false, Ordering::Relaxed);
                } else {
                    data.fill(Out::EQUILIBRIUM);
                    return;
                }
            }

            if !is_buffering && buffered_samples == 0 && !decoder_done {
                state.waiting_for_seek.store(true, Ordering::Relaxed);
                is_buffering = true;
            }

            if is_buffering {
                if buffered_samples >= min_samples_to_resume || decoder_done {
                    state.waiting_for_seek.store(false, Ordering::Relaxed);
                } else {
                    data.fill(Out::EQUILIBRIUM);
                    return;
                }
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
                state
                    .current_frame
                    .fetch_add((samples_read / channels) as u64, Ordering::Relaxed);
            } else if decoder_done && !state.is_finished.swap(true, Ordering::SeqCst) {
                state.finish_notify.notify_waiters();
            }
        },
        |_err| {},
        None,
    )?;
    Ok(stream)
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
            let config: cpal::StreamConfig = c.with_sample_rate(target_sr).into();
            return Ok((config, *fmt));
        }
    }

    if let Some(c) = candidates
        .iter()
        .find(|c| is_supported_output_format(c.sample_format()))
    {
        let config: cpal::StreamConfig = c.with_sample_rate(target_sr).into();
        return Ok((config, c.sample_format()));
    }

    Err("Hardware doesn't support file's sample-rate/channels in compatible sample format".into())
}

pub(crate) fn device_id(device: &cpal::Device) -> String {
    let alsa_name = device
        .name()
        .unwrap_or_else(|_| "Unknown Device".to_string());

    #[cfg(target_os = "linux")]
    {
        use alsa::device_name::HintIter;
        use std::ffi::CStr;

        if let Ok(iter) = HintIter::new(None, unsafe { CStr::from_bytes_with_nul_unchecked(b"pcm\0") }) {
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
    let alsa_name = device
        .name()
        .unwrap_or_else(|_| "Unknown Device".to_string());

    #[cfg(target_os = "linux")]
    {
        use alsa::device_name::HintIter;
        use std::ffi::CStr;

        if let Ok(iter) = HintIter::new(None, unsafe { CStr::from_bytes_with_nul_unchecked(b"pcm\0") }) {
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
    if id.contains("plughw:") {
        return None;
    }
    if let Some(stripped) = id.strip_prefix("alsa:hw:") {
        return Some(format!("plughw:{}", stripped));
    }
    if let Some(stripped) = id.strip_prefix("hw:") {
        return Some(format!("plughw:{}", stripped));
    }
    None
}

pub(crate) fn should_list_linux_output_device(
    id: &str,
    default_id: Option<&str>,
    current_id: Option<&str>,
) -> bool {
    if id == "default" {
        return default_id.map_or(true, |d| d == "default") || current_id.map_or(false, |c| c == "default");
    }

    let is_virtual = id.starts_with("null")
        || id.starts_with("surround")
        || id.contains("upmix")
        || id.contains("vmax");

    if is_virtual {
        return default_id.map_or(false, |d| d == id) || current_id.map_or(false, |c| c == id);
    }

    true
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

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_plughw_locator_rewrites_hw_device_ids() {
        assert_eq!(
            linux_plughw_locator("alsa:hw:CARD=Device,DEV=0").as_deref(),
            Some("plughw:CARD=Device,DEV=0")
        );
        assert_eq!(
            linux_plughw_locator("hw:CARD=Device,DEV=1").as_deref(),
            Some("plughw:CARD=Device,DEV=1")
        );
        assert_eq!(
            linux_plughw_locator("plughw:CARD=Device,DEV=0"),
            None
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
        assert!(should_list_linux_output_device(
            "default",
            Some("default"),
            Some("default")
        ));
        // We now allow pulse and dmix by default
        assert!(should_list_linux_output_device(
            "pulse",
            Some("hw:CARD=0,DEV=0"),
            Some("default")
        ));
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
