use crate::audio::state::SharedState;
use cpal::FromSample;
use cpal::traits::DeviceTrait;
use ringbuf::traits::{Consumer, Observer};
use std::io::Write;
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
    /// 同一端点在各命名空间的全部 id 写法（含 id 本身；虚拟端点只有自身）。
    /// 数字卡号写法在 USB 拔插后会漂移、字符卡名稳定，前端持久化身份与列表
    /// 匹配都要经这份别名表，做不了 /proc/asound 归一的渲染层才不会写法分裂。
    pub aliases: Vec<String>,
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

/// 直通版输出流。dasp 为同一类型提供了恒等的 `FromSample<S> for S` 实现
/// （conv.rs:574），因此这里直接委托给转换版回调，避免两份并发敏感的
/// 回调逻辑（音量、时钟推进、decoder_done/is_finished 判定）各自漂移。
pub(crate) fn build_stream<S, C>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    consumer: C,
    state: Arc<SharedState>,
    channels: usize,
) -> Result<cpal::Stream, Box<dyn std::error::Error>>
where
    S: cpal::SizedSample + cpal::FromSample<S> + Send + 'static,
    C: Consumer<Item = S> + Observer<Item = S> + Send + 'static,
{
    build_stream_converted::<S, S, C>(device, config, consumer, state, channels)
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
    let err_state = state.clone();
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

            // 软件音量：1.0 时完全直通，不触碰样本（BitPerfect 会话恒为 1.0）。
            let volume = state.volume();
            let apply_volume = volume < 1.0;
            let amp = <Out::Float as FromSample<f32>>::from_sample_(volume);

            let mut samples_read = 0usize;
            for sample in data.iter_mut() {
                if let Some(s) = consumer.try_pop() {
                    let converted = Out::from_sample(s);
                    *sample = if apply_volume {
                        converted.mul_amp(amp)
                    } else {
                        converted
                    };
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
            } else if state.decoder_done.load(Ordering::Acquire) {
                // Acquire：同步解码线程「先落 playback_failed 再开 decoder_done」的
                // 写入顺序（见 decoder.rs fail_playback_after_stream_error），保证随后
                // tick() 读到的 is_finished=true 必然携带正确的 playback_failed 值；
                // Relaxed 在弱序目标上会把致命错误误判成自然播完。
                state.is_finished.store(true, Ordering::SeqCst);
            }
        },
        move |err| handle_stream_error(err, &err_state),
        None,
    )?;
    Ok(stream)
}

/// 输出流的致命错误处理。设备被拔出/被抢占后，CPAL 的 ALSA worker 可能会在
/// poll 返回 POLLERR 时进入无休止自旋重试循环。这里通过 atomic swap 保证错误
/// 只记录一次，并通过 writeln! 避免 stderr 管道写满时（os error 11 EAGAIN）
/// 发生 panic 导致音频线程崩溃。
/// 同时通过短休眠抑制 100% CPU 自旋，为外层检测 is_finished 并销毁流留出安全窗口。
/// 设备类错误额外置位 device_lost：worker 据此把会话自动迁移到系统默认续播，
/// 而不是当成普通流错误终止会话（解码器/网络错误不置位，仍走终止路径）。
fn handle_stream_error(err: cpal::StreamError, state: &SharedState) {
    match err {
        cpal::StreamError::DeviceNotAvailable | cpal::StreamError::StreamInvalidated => {
            state.device_lost.store(true, Ordering::SeqCst);
            if !state.playback_failed.swap(true, Ordering::SeqCst) {
                let _ = writeln!(std::io::stderr(), "[audio] output stream lost: {err}");
                state.is_finished.store(true, Ordering::SeqCst);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        // 欠载只是一次爆音，ALSA 自己会恢复，结束会话属于过度反应。
        cpal::StreamError::BufferUnderrun => {}
        // 其余 host 特定错误按致命处理：ALSA 上设备拔出/被抢占往往以裸 errno
        // （ENODEV/EINVAL 等）或 poll POLLERR 的 BackendSpecific 出现而非上面的类型化变体。
        cpal::StreamError::BackendSpecific { err } => {
            state.device_lost.store(true, Ordering::SeqCst);
            if !state.playback_failed.swap(true, Ordering::SeqCst) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[audio] output stream fatal error: {}",
                    err.description
                );
                state.is_finished.store(true, Ordering::SeqCst);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
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
        // clear() 对 Copy 样本就是一次读指针推进（drop 为 no-op 被编译器消除）；
        // 旧的逐样本 try_pop 循环在 seek 时要在音频回调里弹掉最多 6 秒的缓冲，
        // 每个样本一次原子操作，是回调超时(xrun)的隐患。
        consumer.clear();
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

/// 设备 id：后端用来重新打开设备的原始标识（ALSA 上就是 PCM 名，
/// 形如 `hw:CARD=1,DEV=0` / `default` / `pipewire`）。
///
/// 注意不能用 `description().name()`：cpal 0.17 起那是给人看的显示名
/// （USB DAC 上是 "FreeDSP Mini, USB Audio"），拿它当 id 会让
/// `find_output_device` 的等值匹配、`is_current` 判定、以及独占模式的
/// “是否真实硬件端点”校验全部失效。`DeviceId` 的第二个字段才是后端标识，
/// 与旧版 `DeviceTrait::name()`（现已废弃）取到的是同一个串。
pub(crate) fn device_id(device: &cpal::Device) -> String {
    device.id().map(|id| id.1).unwrap_or_default()
}

/// 设备显示名：cpal 已把 ALSA hint 的描述首行（以及物理设备的
/// “声卡名, PCM 名”）整理进 description，无需自己再解析一遍 hint。
pub(crate) fn device_display_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|desc| desc.name().to_string())
        .unwrap_or_else(|_| device_id(device))
}

/// 两个输出设备 id 是否指向同一个可打开端点。
///
/// Linux 上同一物理端点存在两种写法：`hw:CARD=1,DEV=0`（数字卡号，来自
/// /proc/asound 与 cpal 的物理探测）和 `hw:CARD=Mini,DEV=0`（字符卡名，来自
/// ALSA hint），字符串不等但指向同一个端点。`plughw:` 与 `hw:` 端点相同却一个
/// 带软件转换、一个直通，BitPerfect 下不可互换，所以端点键之外还要比对前缀。
pub(crate) fn output_device_ids_match(left: &str, right: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        output_device_ids_match_at(left, right, std::path::Path::new("/proc/asound"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        left == right
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn output_device_ids_match_at(
    left: &str,
    right: &str,
    asound_root: &std::path::Path,
) -> bool {
    if left == right {
        return true;
    }

    match (
        linux_hw_endpoint_key_at(left, asound_root),
        linux_hw_endpoint_key_at(right, asound_root),
    ) {
        (Some(left_key), Some(right_key)) => {
            left_key == right_key
                && is_linux_plughw_output_id(left) == is_linux_plughw_output_id(right)
        }
        _ => false,
    }
}

/// 等待输出设备可被打开。独占播放要先让 PipeWire 交还声卡，释放是异步的：
/// 预留刚拿到手时 `hw:` 往往还是 EBUSY，直接开流就会以“设备不可用”失败。
/// `supported_output_configs()` 内部就是一次 PCM open，正好当作可用性探针。
pub(crate) fn wait_for_output_device_available(
    device: &cpal::Device,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = std::time::Instant::now() + timeout;

    loop {
        match device.supported_output_configs() {
            Ok(_) => return Ok(()),
            Err(cpal::SupportedStreamConfigsError::DeviceNotAvailable) => {
                if std::time::Instant::now() >= deadline {
                    return Err(format!(
                        "无法启用独占输出：设备 {} 正被其他程序占用或已断开，\
                         请关闭占用该设备的程序后重试",
                        device_id(device)
                    )
                    .into());
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            Err(err) => return Err(err.into()),
        }
    }
}

/// 把 hw:/plughw: 设备 id 解析为 (card_index, device_index) 端点键。
/// UI 侧的 id 可能来自 /proc/asound（数字 CARD=1），cpal/ALSA hint 的 id
/// 则用字符卡名（CARD=UsbDac）；两种写法字符串不相等但指向同一物理端点，
/// 必须归一到数字索引后再比较。
#[cfg(target_os = "linux")]
pub(crate) fn linux_hw_endpoint_key(id: &str) -> Option<(u32, u32)> {
    linux_hw_endpoint_key_at(id, std::path::Path::new("/proc/asound"))
}

#[cfg(target_os = "linux")]
pub(crate) fn linux_hw_endpoint_key_at(
    id: &str,
    asound_root: &std::path::Path,
) -> Option<(u32, u32)> {
    if !is_linux_real_hardware_output_id(id) {
        return None;
    }
    let card_index = alsa_card_index_from_device_id_at(id, asound_root)?;
    let device_index = extract_alsa_device_token(id).unwrap_or(0);
    Some((card_index, device_index))
}

#[cfg(target_os = "linux")]
pub(crate) fn is_linux_plughw_output_id(id: &str) -> bool {
    id.trim()
        .strip_prefix("alsa:")
        .unwrap_or_else(|| id.trim())
        .trim()
        .to_ascii_lowercase()
        .starts_with("plughw:")
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

        // 规范写法：字符卡名跨拔插稳定（数字卡号会随 USB 枚举顺序漂移），
        // 由它充当条目 id，持久化身份才不会插回一次就永久失效。
        let numeric_id = format!("hw:CARD={},DEV={}", card.index, device_index);
        let id = canonical_linux_output_device_id_at(&numeric_id, asound_root);
        let name = format!("{} - {}", card.display_name(), pcm_name);
        // 归一匹配必须用同一份 asound_root（测试传的是临时目录；不带 _at 的
        // 变体会读真实 /proc/asound，规范化 id 在测试环境里永远归一不上）。
        let is_default =
            default_id.is_some_and(|value| output_device_ids_match_at(value, &id, asound_root));
        let is_current =
            current_id.is_some_and(|value| output_device_ids_match_at(value, &id, asound_root));

        if let Some(device) = devices.iter_mut().find(|device| device.id == id) {
            if device.name == device.id || device.name.trim().is_empty() {
                device.name = name;
            }
            device.is_default |= is_default;
            device.is_current |= is_current;
            continue;
        }

        devices.push(OutputDeviceInfo {
            is_default,
            is_current,
            aliases: linux_output_device_id_aliases_at(&id, asound_root),
            id,
            name,
        });
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn collapse_linux_duplicate_output_devices(devices: &mut Vec<OutputDeviceInfo>) {
    collapse_linux_duplicate_output_devices_at(devices, std::path::Path::new("/proc/asound"));
}

/// 同一物理端点会以多种 id 出现（cpal hint 的字符卡名 hw:/plughw:、
/// /proc/asound 与 cpal 物理探测的数字卡号），按 (card, device) 端点键合并成
/// 一条：条目 id 规范化为字符卡名写法（跨拔插稳定，数字卡号会随 USB 枚举
/// 顺序漂移），别名为全写法集合——前端的持久化身份与激活匹配改走别名表，
/// 不再依赖“合并后条目的 id 恰好等于前端持久化写法”的巧合；显示名取非
/// 占位的一侧，hw:/plughw: 属不同输出端点仍互不合并。
#[cfg(target_os = "linux")]
pub(crate) fn collapse_linux_duplicate_output_devices_at(
    devices: &mut Vec<OutputDeviceInfo>,
    asound_root: &std::path::Path,
) {
    use std::collections::HashMap;

    let mut kept_by_key: HashMap<(u32, u32), usize> = HashMap::new();
    let mut result: Vec<OutputDeviceInfo> = Vec::new();

    for device in devices.drain(..) {
        let Some(key) = linux_hw_endpoint_key_at(&device.id, asound_root) else {
            let mut device = device;
            if device.aliases.is_empty() {
                device.aliases = linux_output_device_id_aliases_at(&device.id, asound_root);
            }
            result.push(device);
            continue;
        };

        let canonical_id = canonical_linux_output_device_id_at(&device.id, asound_root);

        let Some(&index) = kept_by_key.get(&key) else {
            let aliases = linux_output_device_id_aliases_at(&canonical_id, asound_root);
            let mut device = device;
            device.id = canonical_id;
            if device.aliases.is_empty() {
                device.aliases = aliases;
            } else {
                merge_aliases(&mut device.aliases, &aliases);
            }
            kept_by_key.insert(key, result.len());
            result.push(device);
            continue;
        };

        let incoming_aliases = linux_output_device_id_aliases_at(&canonical_id, asound_root);
        let existing = &mut result[index];
        let existing_placeholder = existing.name == existing.id || existing.name.trim().is_empty();
        let incoming_placeholder = device.name == device.id || device.name.trim().is_empty();

        // 数字卡号条目的名字多半来自 /proc 追加（已填真实显示名），优先于
        // cpal 物理探测的占位名。
        if existing_placeholder && !incoming_placeholder {
            existing.name = device.name.clone();
        }

        // 合并后的 id 固定用规范写法；current 标记与别名为并集。
        existing.id = canonical_linux_output_device_id_at(&existing.id, asound_root);
        let existing_aliases = linux_output_device_id_aliases_at(&existing.id, asound_root);
        merge_aliases(&mut existing.aliases, &existing_aliases);
        merge_aliases(&mut existing.aliases, &incoming_aliases);
        existing.is_default |= device.is_default;
        existing.is_current |= device.is_current;
    }

    *devices = result;
}

fn merge_aliases(target: &mut Vec<String>, extra: &[String]) {
    for alias in extra {
        if !target.contains(alias) {
            target.push(alias.clone());
        }
    }
}

pub(crate) fn is_linux_real_hardware_output_id(id: &str) -> bool {
    let normalized = id.trim().to_ascii_lowercase();
    normalized.starts_with("hw:")
        || normalized.starts_with("plughw:")
        || normalized.starts_with("alsa:hw:")
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
fn alsa_card_id_from_index_at(card_index: u32, asound_root: &std::path::Path) -> Option<String> {
    let card_id_path = asound_root.join(format!("card{card_index}")).join("id");
    let card_id = std::fs::read_to_string(card_id_path).ok()?;
    let trimmed = card_id.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// 把设备 id 规范化为跨拔插稳定的写法：hw:/plughw: 端点的数字卡号换成字符
/// 卡名（`hw:CARD=1,DEV=0` → `hw:CARD=UsbDac,DEV=0`），其余原样返回。
///
/// 数字卡号在拔插后可能漂移（USB 枚举顺序变化），拿它做持久化身份，插回一次
/// 就永久失效；字符卡名由 ALSA 按卡身份解析，插回后哪怕卡号变了仍能打开。
/// 无法解析字符卡名（读不到 /proc/asound）时返回原 id——稳定写法不可得时，
/// 至少不能让枚举凭空丢设备。
#[cfg(target_os = "linux")]
pub(crate) fn canonical_linux_output_device_id_at(
    id: &str,
    asound_root: &std::path::Path,
) -> String {
    if !is_linux_real_hardware_output_id(id) {
        return id.to_string();
    }

    let Some(card_index) = alsa_card_index_from_device_id_at(id, asound_root) else {
        return id.to_string();
    };
    // 已是字符写法的不再翻回数字：目标是稳定，不是统一形状。
    let card_token = extract_alsa_card_token(id);
    if card_token
        .as_deref()
        .and_then(|token| token.parse::<u32>().ok())
        .is_none()
    {
        return id.to_string();
    }

    let Some(card_id) = alsa_card_id_from_index_at(card_index, asound_root) else {
        return id.to_string();
    };

    id.replace(&format!("CARD={card_index}"), &format!("CARD={card_id}"))
}

/// 同一物理端点在各命名空间的全部写法（去重）。前端拿它与持久化的 id 做匹配，
/// 弥补渲染层做不了 /proc/asound 归一比较的缺口：hw:CARD=1,DEV=0（数字卡号，
/// cpal 物理探测 / /proc/asound）、hw:CARD=UsbDac,DEV=0（字符卡名，ALSA hint）
/// 以及 plughw: 变体。注意 hw:/plughw: 是不同输出端点（一个直通一个带软件
/// 转换），互不进对方的别名表。
#[cfg(target_os = "linux")]
pub(crate) fn linux_output_device_id_aliases_at(
    id: &str,
    asound_root: &std::path::Path,
) -> Vec<String> {
    let mut aliases = vec![id.to_string()];
    if !is_linux_real_hardware_output_id(id) {
        return aliases;
    }

    let Some(card_index) = alsa_card_index_from_device_id_at(id, asound_root) else {
        return aliases;
    };
    let card_tokens: [String; 2] = [
        card_index.to_string(),
        alsa_card_id_from_index_at(card_index, asound_root)
            .unwrap_or_else(|| card_index.to_string()),
    ];

    let prefix = if is_linux_plughw_output_id(id) {
        "plughw"
    } else {
        "hw"
    };
    let device_index = extract_alsa_device_token(id).unwrap_or(0);

    for token in card_tokens {
        let alias = format!("{prefix}:CARD={token},DEV={device_index}");
        if !aliases.contains(&alias) {
            aliases.push(alias);
        }
    }

    aliases
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
            playback_failed: AtomicBool::new(false),
            buffered_frames: AtomicU64::new(0),
            waiting_for_seek: AtomicBool::new(false),
            volume_bits: AtomicU32::new(crate::audio::state::UNITY_VOLUME_BITS),
            volume_locked: AtomicBool::new(false),
            device_lost: AtomicBool::new(false),
        }
    }

    /// 建一个带 card{index}/id = {card_id} 的临时 /proc/asound 根，供端点键
    /// 匹配类测试使用。测试并行运行，序号保证各次调用的目录互不相撞；
    /// 调用方在断言结束后自行 remove_dir_all 清理。
    #[cfg(target_os = "linux")]
    fn temp_asound_root_with_card(card_index: u32, card_id: &str) -> std::path::PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ncm-asound-helper-test-{}-{seq}",
            std::process::id()
        ));
        let card_dir = root.join(format!("card{card_index}"));
        std::fs::create_dir_all(&card_dir).unwrap();
        std::fs::write(card_dir.join("id"), format!("{card_id}\n")).unwrap();
        root
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
    fn linux_hw_endpoint_key_matches_named_and_numeric_ids_to_same_endpoint() {
        let root = temp_asound_root_with_card(1, "UsbDac");

        // /proc/asound 列表用数字 CARD，cpal/ALSA hint 用字符卡名——两者必须
        // 解析到同一个端点键，否则 UI 选中的 USB 设备永远无法在 cpal 中命中。
        assert_eq!(
            linux_hw_endpoint_key_at("hw:CARD=1,DEV=0", &root),
            Some((1, 0))
        );
        assert_eq!(
            linux_hw_endpoint_key_at("hw:CARD=UsbDac,DEV=0", &root),
            Some((1, 0))
        );
        assert_eq!(
            linux_hw_endpoint_key_at("plughw:CARD=UsbDac,DEV=2", &root),
            Some((1, 2))
        );
        assert_eq!(
            linux_hw_endpoint_key_at("alsa:hw:CARD=UsbDac,DEV=0", &root),
            Some((1, 0))
        );
        assert_eq!(linux_hw_endpoint_key_at("default", &root), None);
        assert_eq!(linux_hw_endpoint_key_at("pipewire", &root), None);

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn plughw_ids_are_detected_for_flavor_preference() {
        assert!(is_linux_plughw_output_id("plughw:CARD=UsbDac,DEV=0"));
        assert!(is_linux_plughw_output_id("alsa:plughw:CARD=3,DEV=0"));
        assert!(!is_linux_plughw_output_id("hw:CARD=UsbDac,DEV=0"));
        assert!(!is_linux_plughw_output_id("default"));
    }

    /// 回归：cpal 枚举出的设备 id 必须和 /proc/asound 报告的 hw: 端点对得上。
    ///
    /// `device_id` 一度错用 `description().name()`——那是给人看的显示名
    /// （"FreeDSP Mini, USB Audio"），于是 cpal 侧的 id 和 UI 列表里来自
    /// /proc/asound 的 `hw:CARD=1,DEV=0` 没有一条能对上：`find_output_device`
    /// 精确匹配和端点键匹配双双落空，选任何设备都报 Device not found；
    /// 独占模式的“是否真实硬件端点”校验也因此恒为假而拒绝播放。
    #[cfg(target_os = "linux")]
    #[test]
    fn cpal_device_ids_line_up_with_proc_asound_hw_endpoints() {
        let mut expected = Vec::new();
        append_linux_proc_asound_output_devices(&mut expected, None, None);
        if expected.is_empty() {
            return; // 无 /proc/asound 的构建环境（容器 / 非 Linux CI）
        }

        let host = cpal::default_host();
        let Ok(devices) = cpal::traits::HostTrait::output_devices(&host) else {
            return;
        };
        let enumerated: Vec<String> = devices.map(|device| device_id(&device)).collect();

        for device in expected {
            assert!(
                enumerated
                    .iter()
                    .any(|id| output_device_ids_match(id, &device.id)),
                "cpal 没有枚举出 /proc/asound 报告的端点 {}；实际枚举到：{:?}",
                device.id,
                enumerated
            );
        }
    }

    /// 同一端点的数字卡号与字符卡名写法必须判定为同一设备，否则每次播放前的
    /// “设备是否已生效”检查都会误判成需要切换，白白重建整条播放链路；
    /// 而 hw: 与 plughw: 端点相同但一个直通一个带软件转换，BitPerfect 下
    /// 不能混为一谈。
    #[cfg(target_os = "linux")]
    #[test]
    fn output_device_ids_match_across_id_spellings_but_not_across_plug_flavor() {
        let root = temp_asound_root_with_card(1, "UsbDac");

        // 数字卡号（/proc/asound、cpal 物理探测）与字符卡名（ALSA hint）等价。
        assert!(output_device_ids_match_at(
            "hw:CARD=1,DEV=0",
            "hw:CARD=UsbDac,DEV=0",
            &root
        ));
        // 端点相同但一个直通一个带软件转换，BitPerfect 下不可互换。
        assert!(!output_device_ids_match_at(
            "hw:CARD=1,DEV=0",
            "plughw:CARD=UsbDac,DEV=0",
            &root
        ));
        // 不同 PCM 设备号是不同端点。
        assert!(!output_device_ids_match_at(
            "hw:CARD=1,DEV=0",
            "hw:CARD=1,DEV=2",
            &root
        ));
        // 虚拟端点只有字符串等值这一条路径。
        assert!(output_device_ids_match_at("default", "default", &root));
        assert!(!output_device_ids_match_at("default", "pipewire", &root));
        // 显示名不是 id：不能让它和任何真实端点判成同一个设备。
        assert!(!output_device_ids_match_at(
            "hw:CARD=1,DEV=0",
            "FreeDSP Mini, USB Audio",
            &root
        ));

        let _ = std::fs::remove_dir_all(&root);
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
        let root = temp_asound_root_with_card(2, "UsbDac");

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
    fn proc_asound_fallback_lists_real_playback_pcm_devices() {
        let root = temp_asound_root_with_card(0, "PCH");
        std::fs::create_dir_all(root.join("card1")).unwrap();
        std::fs::write(root.join("card1").join("id"), "UsbDac\n").unwrap();
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

        // id 规范化为字符卡名写法（跨拔插稳定），数字卡号写法进别名表。
        assert!(devices.iter().any(|device| {
            device.id == "hw:CARD=UsbDac,DEV=0"
                && device.name == "Reference USB DAC - USB Audio"
                && device.is_current
                && device.aliases.contains(&"hw:CARD=1,DEV=0".to_string())
        }));
        assert!(
            devices
                .iter()
                .any(|device| device.id == "hw:CARD=PCH,DEV=3")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn proc_asound_fallback_updates_existing_hw_names_and_collapses_plughw_twins() {
        let root = temp_asound_root_with_card(1, "UsbDac");
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
                id: "hw:CARD=UsbDac,DEV=0".to_string(),
                aliases: vec!["hw:CARD=UsbDac,DEV=0".to_string()],
                name: "hw:CARD=UsbDac,DEV=0".to_string(),
                is_default: false,
                is_current: false,
            },
            OutputDeviceInfo {
                id: "plughw:CARD=UsbDac,DEV=0".to_string(),
                aliases: vec!["plughw:CARD=UsbDac,DEV=0".to_string()],
                name: "plughw:CARD=UsbDac,DEV=0".to_string(),
                is_default: false,
                is_current: false,
            },
        ];

        // /proc/asound 追加数字卡号写法的同一端点；三条记录必须合并成一条。
        append_linux_proc_asound_output_devices_at(&mut devices, None, None, &root);
        collapse_linux_duplicate_output_devices_at(&mut devices, &root);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "hw:CARD=UsbDac,DEV=0");
        assert_eq!(devices[0].name, "Reference USB DAC - USB Audio");

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn collapse_keeps_the_id_carrying_is_current_so_active_matching_stays_stable() {
        let root = temp_asound_root_with_card(1, "UsbDac");

        let mut devices = vec![
            OutputDeviceInfo {
                id: "hw:CARD=UsbDac,DEV=0".to_string(),
                aliases: vec!["hw:CARD=UsbDac,DEV=0".to_string()],
                name: "Reference USB DAC, USB Audio".to_string(),
                is_default: false,
                is_current: false,
            },
            // UI 持久化的可能是数字卡号写法；合并后的条目 id 固定为规范
            //（字符卡名）写法，数字写法进别名表——前端经别名表匹配持久化
            // id，不再依赖“条目 id 恰好等于持久化写法”的巧合。
            OutputDeviceInfo {
                id: "hw:CARD=1,DEV=0".to_string(),
                aliases: vec!["hw:CARD=1,DEV=0".to_string()],
                name: "Reference USB DAC - USB Audio".to_string(),
                is_default: false,
                is_current: true,
            },
        ];

        collapse_linux_duplicate_output_devices_at(&mut devices, &root);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "hw:CARD=UsbDac,DEV=0");
        assert!(devices[0].is_current);
        assert!(devices[0].aliases.contains(&"hw:CARD=1,DEV=0".to_string()));
        assert!(
            devices[0]
                .aliases
                .contains(&"hw:CARD=UsbDac,DEV=0".to_string())
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_output_device_names_are_disambiguated_with_ids() {
        let mut devices = vec![
            OutputDeviceInfo {
                id: "hw:CARD=0,DEV=0".to_string(),
                aliases: vec!["hw:CARD=0,DEV=0".to_string()],
                name: "USB DAC".to_string(),
                is_default: false,
                is_current: false,
            },
            OutputDeviceInfo {
                id: "hw:CARD=1,DEV=0".to_string(),
                aliases: vec!["hw:CARD=1,DEV=0".to_string()],
                name: "USB DAC".to_string(),
                is_default: false,
                is_current: true,
            },
            OutputDeviceInfo {
                id: "default".to_string(),
                aliases: vec!["default".to_string()],
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

    #[test]
    fn handle_stream_error_records_failure_and_is_idempotent() {
        let state = Arc::new(SharedState {
            is_paused: std::sync::atomic::AtomicBool::new(false),
            current_frame: std::sync::atomic::AtomicU64::new(0),
            playback_clock: std::sync::Mutex::new(crate::audio::state::PlaybackClock::new()),
            trim_until_frame: std::sync::atomic::AtomicU64::new(crate::audio::state::NO_TRIM_FRAME),
            sample_rate: std::sync::atomic::AtomicU32::new(44100),
            has_seek_request: std::sync::atomic::AtomicBool::new(false),
            seek_request: std::sync::Mutex::new(None),
            is_terminating: std::sync::atomic::AtomicBool::new(false),
            discard_buffer: std::sync::atomic::AtomicBool::new(false),
            is_discarding_buffer: std::sync::atomic::AtomicBool::new(false),
            decoder_done: std::sync::atomic::AtomicBool::new(false),
            is_finished: std::sync::atomic::AtomicBool::new(false),
            playback_failed: std::sync::atomic::AtomicBool::new(false),
            buffered_frames: std::sync::atomic::AtomicU64::new(0),
            waiting_for_seek: std::sync::atomic::AtomicBool::new(false),
            volume_bits: std::sync::atomic::AtomicU32::new(crate::audio::state::UNITY_VOLUME_BITS),
            volume_locked: std::sync::atomic::AtomicBool::new(false),
            device_lost: std::sync::atomic::AtomicBool::new(false),
        });

        let err = cpal::StreamError::BackendSpecific {
            err: cpal::BackendSpecificError {
                description: "`alsa::poll()` returned POLLERR".to_string(),
            },
        };

        handle_stream_error(err, &state);
        assert!(state.playback_failed.load(Ordering::SeqCst));
        assert!(state.is_finished.load(Ordering::SeqCst));

        // 第二次调用不应 panic，并且状态保持一致
        let err2 = cpal::StreamError::BackendSpecific {
            err: cpal::BackendSpecificError {
                description: "`alsa::poll()` returned POLLERR".to_string(),
            },
        };
        handle_stream_error(err2, &state);
        assert!(state.playback_failed.load(Ordering::SeqCst));
        assert!(state.is_finished.load(Ordering::SeqCst));
    }
}
