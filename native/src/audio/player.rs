#[cfg(target_os = "linux")]
use crate::audio::device_reservation::DeviceReservation;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;
use ringbuf::traits::{Producer, Split};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use std::time::Duration;
use symphonia::core::conv::ConvertibleSample;

use crate::audio::decoder::{self, AudioMetadata};
use crate::audio::downloads as builds;
use crate::audio::output::{self, OutputDeviceInfo};
use crate::audio::session::{Session, SessionOutcome};
use crate::audio::source::SeekableSource;
use crate::diagnostics::{record, stage_error};
use serde_json::json;

const OUTPUT_BUFFER_SECONDS: usize = 6;
const INITIAL_PREDECODE_SECONDS: usize = 2;
/// 独占播放拿到设备预留后，等待 PipeWire 交还声卡的上限。
#[cfg(target_os = "linux")]
const DEVICE_RELEASE_TIMEOUT: Duration = Duration::from_millis(2_000);
/// 安全包装 cpal::Stream，隔离 drop 期间可能的底层 panic。
/// 在 Linux ALSA 下，若音频工作线程（cpal_alsa_out）因错误退出，
/// cpal 的 Stream::drop 会在向已关闭的唤醒管道写入时断言失败（assert_eq!(ret, 8)）。
/// 通过 catch_unwind 捕获并吸收该 panic，避免连带导致宿主工作线程（native-worker）崩溃。
pub(crate) struct SafeStream(Option<cpal::Stream>);

impl SafeStream {
    pub(crate) fn new(stream: cpal::Stream) -> Self {
        Self(Some(stream))
    }

    pub(crate) fn play(&self) -> Result<(), cpal::PlayStreamError> {
        self.0
            .as_ref()
            .expect("SafeStream inner stream is None")
            .play()
    }
}

impl Drop for SafeStream {
    fn drop(&mut self) {
        if let Some(stream) = self.0.take()
            && let Err(err) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                drop(stream);
            }))
        {
            let _ = writeln!(
                std::io::stderr(),
                "[audio] warning: cpal output stream drop panicked (worker thread may have exited early): {:?}",
                err
            );
        }
    }
}

pub struct AudioPlayer {
    device: cpal::Device,
    requested_device_id: Option<String>,
    stream: Option<SafeStream>,
    /// 当前播放会话。`None` = 还没开播过（或已 stop），此时读进度得到 0、
    /// 读会话状态得到"无会话"的默认值。
    session: Option<Arc<Session>>,
    /// 当前会话的解码线程。换会话/停止时先终结会话再 join，避免旧线程在下一次
    /// 开播时还与新的输出回调共用同一份状态（旧线程只认自己那份，但会多占一个
    /// ring buffer 的写端与一份解码器）。
    decode_thread: Option<std::thread::JoinHandle<()>>,
    /// 开播音量种子：worker 在每次 Play 前用渲染层最新下发的音量同步它，
    /// setup_and_play 把它种进新会话（BitPerfect 会话忽略、强制 1.0 直通）。
    /// 会话进行中的 SetVolume 也写这里，供同实例上的下一次开播继承。
    volume_seed: f32,
    #[cfg(target_os = "linux")]
    device_reservation: Option<DeviceReservation>,
}

impl AudioPlayer {
    pub fn new(device_name: Option<&str>) -> Result<Self, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        // 记录的是解析后设备的真实 id，而不是调用方传进来的写法：同一端点有
        // hw:CARD=1,DEV=0 与 hw:CARD=Mini,DEV=0 两种等价写法，存原样会让
        // output_devices() 的 is_current 永远匹配不上枚举出来的那一条。
        let (device, requested_device_id) = match device_name {
            Some(name) => {
                let device = Self::find_output_device(&host, name)?;
                let id = output::device_id(&device);
                (device, Some(id))
            }
            None => (
                host.default_output_device()
                    .ok_or("No default output device found")?,
                None,
            ),
        };

        Ok(Self {
            device,
            requested_device_id,
            stream: None,
            session: None,
            decode_thread: None,
            volume_seed: 1.0,
            #[cfg(target_os = "linux")]
            device_reservation: None,
        })
    }

    fn find_output_device(
        host: &cpal::Host,
        name: &str,
    ) -> Result<cpal::Device, Box<dyn std::error::Error>> {
        let mut devices: Vec<cpal::Device> = host.output_devices()?.collect();

        // 精确 id 命中优先（"default" 等虚拟端点只有这一条匹配路径）。
        if let Some(position) = devices
            .iter()
            .position(|device| output::device_id(device) == name)
        {
            return Ok(devices.swap_remove(position));
        }

        // UI 侧的 hw id 可能用数字 CARD（来自 /proc/asound），cpal 枚举出的
        // ALSA hint id 用字符卡名，字符串比较永远失配；归一成
        // (card_index, device_index) 端点键再匹配，hw/plughw 同型优先，
        // 保证 BitPerfect 选中的 hw: 不被 plughw: 顶替。
        #[cfg(target_os = "linux")]
        if let Some(target_key) = output::linux_hw_endpoint_key(name) {
            let wants_plug = output::is_linux_plughw_output_id(name);
            let mut fallback = None;
            for (index, device) in devices.iter().enumerate() {
                let id = output::device_id(device);
                if output::linux_hw_endpoint_key(&id) != Some(target_key) {
                    continue;
                }
                if output::is_linux_plughw_output_id(&id) == wants_plug {
                    return Ok(devices.swap_remove(index));
                }
                if fallback.is_none() {
                    fallback = Some(index);
                }
            }
            if let Some(index) = fallback {
                return Ok(devices.swap_remove(index));
            }
        }

        Err(format!("Device not found: {}", name).into())
    }

    pub fn output_devices(&self) -> Result<Vec<OutputDeviceInfo>, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let default_device = host.default_output_device();
        let default_id = default_device.as_ref().map(output::device_id);

        let mut devices = Vec::new();
        for d in host.output_devices()? {
            let id = output::device_id(&d);
            let name = output::device_display_name(&d);

            #[cfg(target_os = "linux")]
            if !output::should_list_linux_output_device(
                &id,
                default_id.as_deref(),
                self.requested_device_id.as_deref(),
            ) {
                continue;
            }

            let is_default = default_id
                .as_deref()
                .is_some_and(|value| output::output_device_ids_match(value, &id));
            let is_current = match self.requested_device_id.as_deref() {
                Some(rid) => output::output_device_ids_match(rid, &id),
                None => is_default,
            };

            devices.push(OutputDeviceInfo {
                #[cfg(target_os = "linux")]
                aliases: Vec::new(),
                #[cfg(not(target_os = "linux"))]
                aliases: vec![id.clone()],
                id: id.clone(),
                name,
                is_default,
                is_current,
            });
        }
        #[cfg(target_os = "linux")]
        output::append_linux_proc_asound_output_devices(
            &mut devices,
            default_id.as_deref(),
            self.requested_device_id.as_deref(),
        );

        #[cfg(target_os = "linux")]
        output::collapse_linux_duplicate_output_devices(&mut devices);
        output::disambiguate_output_device_names(&mut devices);
        Ok(devices)
    }

    pub async fn play_url(
        &mut self,
        url: &str,
        start_at: Option<Duration>,
        strict_bit_perfect: bool,
        exclusive: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let extension = Path::new(url)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let reader = builds::build_uncached_stream_download(url)
            .await
            .map_err(|error| stage_error("建立音源下载流", error.as_ref()))?;

        let content_len = reader.content_length();
        let source = Box::new(SeekableSource::new(reader, content_len));
        let meta = decoder::spawn_probe_task(source, extension)
            .await
            .map_err(|error| stage_error("识别音源格式", error.as_ref()))?;
        self.setup_and_play(meta, start_at, strict_bit_perfect, exclusive)
            .map_err(|error| stage_error("启动音频输出", error.as_ref()))
    }

    pub async fn play_url_cached(
        &mut self,
        url: &str,
        cache_path: &str,
        metadata_path: &str,
        duration_ms: Option<u64>,
        cache_ahead_secs: Option<u32>,
        max_cache_ahead_bytes: Option<u64>,
        start_at: Option<Duration>,
        strict_bit_perfect: bool,
        exclusive: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let extension = Path::new(url)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let download = builds::build_cached_stream_download(
            url,
            cache_path,
            metadata_path,
            duration_ms,
            cache_ahead_secs,
            max_cache_ahead_bytes,
        )
        .await
        .map_err(|error| stage_error("建立歌曲缓存流", error.as_ref()))?;

        let reader = download.reader;
        let content_len = reader.content_length();
        let source = Box::new(
            SeekableSource::new(reader, content_len).with_storage_state(download.storage_state),
        );
        let meta = decoder::spawn_probe_task(source, extension)
            .await
            .map_err(|error| stage_error("识别缓存音源格式", error.as_ref()))?;
        self.setup_and_play(meta, start_at, strict_bit_perfect, exclusive)
            .map_err(|error| stage_error("启动音频输出", error.as_ref()))
    }

    pub async fn play_file(
        &mut self,
        path: &str,
        start_at: Option<Duration>,
        strict_bit_perfect: bool,
        exclusive: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path_buf = Path::new(path);
        let extension = path_buf
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let file =
            std::fs::File::open(path_buf).map_err(|error| stage_error("打开音频文件", &error))?;
        let source = Box::new(file);

        let meta = decoder::spawn_probe_task(source, extension)
            .await
            .map_err(|error| stage_error("识别音频文件格式", error.as_ref()))?;
        self.setup_and_play(meta, start_at, strict_bit_perfect, exclusive)
            .map_err(|error| stage_error("启动音频输出", error.as_ref()))
    }

    pub(crate) fn setup_and_play(
        &mut self,
        meta: AudioMetadata,
        start_at: Option<Duration>,
        strict_bit_perfect: bool,
        exclusive: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 跟随系统默认时，开播前重新对齐系统最新的默认输出端点（如拔出耳机后默认设备已变为扬声器）。
        if self.requested_device_id.is_none() {
            let host = cpal::default_host();
            if let Some(default) = host.default_output_device() {
                self.device = default;
            }
        }

        // 独占输出（BitPerfect 隐含）目前仅在 Linux（PipeWire pw-reserve）上实现，
        // 其余平台忽略该请求；want_exclusive 在非 Linux 下仅用于禁用默认端点回退。
        let want_exclusive = strict_bit_perfect || exclusive;
        #[cfg(target_os = "linux")]
        let active_device_id = output::device_id(&self.device);

        // stop() 会连同设备预留一起释放。切歌时目标设备没变，预留必须原样留住：
        // 一旦松手，PipeWire 会立刻抢回声卡，下一首重新预留期间的空窗足以让
        // hw: 打开失败——表现就是“独占播放第一首正常，之后随机失败”。
        #[cfg(target_os = "linux")]
        let reusable_reservation = self
            .device_reservation
            .take()
            .filter(|reservation| want_exclusive && reservation.covers_device(&active_device_id));

        self.stop();

        // 新会话的音量取开播前的种子（worker 在每次 Play 前用渲染层最新值同步）；
        // BitPerfect 会话锁定为 1.0 直通，样本不经任何数字增益。
        let session_volume = if strict_bit_perfect {
            1.0
        } else {
            self.volume_seed
        };
        let sr = meta.sample_rate;
        let channels = meta.channels;
        record(
            "info",
            "audio.source_probed",
            json!({
                "device": output::device_id(&self.device), "sample_rate": sr,
                "channels": channels, "bits_per_sample": meta.bits_per_sample,
                "sample_format": format!("{:?}", meta.sample_format),
                "start_ms": start_at.unwrap_or_default().as_millis(),
                "strict": strict_bit_perfect, "exclusive": want_exclusive,
            }),
        );
        // 会话先建起来但不发布（`self.session` 在开流成功后才赋值）：开流失败的
        // 路径上，外部看到的仍是"没有会话"，不会误读一个半初始化的会话。
        // 起点直接落在 start_at 上：开播不是一次 seek，不需要换代或清缓冲。
        let session = Arc::new(Session::new(
            sr,
            channels,
            session_volume,
            strict_bit_perfect,
            start_at.unwrap_or(Duration::ZERO),
        ));

        let should_predecode = start_at.is_none_or(|target| target.is_zero());

        // 独占要求输出落在真实硬件端点上：虚拟端点（default/pipewire/pulse）本身
        // 就是混音服务器的入口，预留它毫无意义，也谈不上独占。
        #[cfg(target_os = "linux")]
        if want_exclusive && !output::is_linux_real_hardware_output_id(&active_device_id) {
            let requirement = if strict_bit_perfect {
                "当前无法满足BitPerfect条件拒绝播放"
            } else {
                "当前无法满足独占输出条件拒绝播放"
            };
            return Err(format!(
                "{}：当前输出端点 {} 不是真实硬件设备，请在设置中选择一个硬件输出设备",
                requirement, active_device_id
            )
            .into());
        }

        // BitPerfect 还必须落在 hw: 上。plughw: 是同一块硬件外面套的转换插件，
        // 它会把任意采样率/位深都报成“支持”，find_bit_perfect_config 于是必然
        // 通过，实际却由 ALSA 悄悄重采样、改位深——校验全过而声音早已不是原始
        // 比特流。这种“假 BitPerfect”比直接拒绝更糟，必须挡在开流之前。
        #[cfg(target_os = "linux")]
        if strict_bit_perfect && output::is_linux_plughw_output_id(&active_device_id) {
            return Err(format!(
                "当前无法满足BitPerfect条件拒绝播放：{} 是带软件转换的 plughw 端点，\
                 会静默重采样，请改选对应的 hw: 直通端点",
                active_device_id
            )
            .into());
        }

        // 预留 → 等待 PipeWire 真正交出声卡 → 再开流。释放是异步的，
        // 拿到预留的瞬间 hw: 往往仍是 EBUSY，不等就会直接开流失败。
        #[cfg(target_os = "linux")]
        let device_reservation = if want_exclusive {
            let reservation = match reusable_reservation {
                Some(reservation) => reservation,
                None => DeviceReservation::reserve(&active_device_id)?,
            };
            output::wait_for_output_device_available(&self.device, DEVICE_RELEASE_TIMEOUT)?;
            Some(reservation)
        } else {
            None
        };

        let (config, sample_format) = if strict_bit_perfect {
            output::find_bit_perfect_config(
                &self.device,
                sr,
                channels,
                meta.bits_per_sample,
                meta.sample_format,
            )?
        } else {
            match output::find_best_config(
                &self.device,
                sr,
                channels,
                meta.bits_per_sample,
                meta.sample_format,
            ) {
                Ok(cfg) => cfg,
                Err(primary_err) => {
                    let primary_msg = primary_err.to_string();
                    // 独占会话不允许悄悄回退到系统默认端点：默认端点不是硬件
                    // 设备，回退等于独占失效，必须把错误如实抛给前端。
                    // （独占仅在 Linux 实现，其他平台始终允许回退。）
                    let allow_fallback = !want_exclusive || !cfg!(target_os = "linux");
                    if allow_fallback && self.maybe_fallback_to_default_device()? {
                        output::find_best_config(
                            &self.device,
                            sr,
                            channels,
                            meta.bits_per_sample,
                            meta.sample_format,
                        )
                        .map_err(|fallback_err| {
                            format!(
                                "No compatible output config: primary={}; fallback={}",
                                primary_msg, fallback_err
                            )
                        })?
                    } else {
                        // 找不到兼容配置可能是设备真不支持，也可能是设备此刻
                        // 已经不在了（拔出瞬间的枚举残留）：find_best_config 的
                        // “不支持采样率/声道”文案会把后者误报成格式问题。抛
                        // 错误前先探一次 PCM 可用性，设备不在就按设备错误报。
                        if let Err(availability_err) = self.device.supported_output_configs() {
                            return Err(format!(
                                "无法在独占输出设备 {} 上开流：{}",
                                output::device_id(&self.device),
                                availability_err
                            )
                            .into());
                        }
                        return Err(primary_msg.into());
                    }
                }
            }
        };

        record(
            "info",
            "audio.output_configured",
            json!({
                "device": output::device_id(&self.device), "config": format!("{config:?}"),
                "sample_format": format!("{sample_format:?}"),
            }),
        );
        let mut decode_thread = None;
        let stream = match sample_format {
            cpal::SampleFormat::I16 => self.build_pipeline::<i16, i16>(
                &config,
                meta,
                should_predecode,
                &mut decode_thread,
                &session,
            )?,
            cpal::SampleFormat::U16 => self.build_pipeline::<u16, u16>(
                &config,
                meta,
                should_predecode,
                &mut decode_thread,
                &session,
            )?,
            cpal::SampleFormat::I8 => self.build_pipeline::<i8, i8>(
                &config,
                meta,
                should_predecode,
                &mut decode_thread,
                &session,
            )?,
            cpal::SampleFormat::U8 => self.build_pipeline::<u8, u8>(
                &config,
                meta,
                should_predecode,
                &mut decode_thread,
                &session,
            )?,
            // ALSA 的 24 位容器以 32 位样本承载：解码侧按 i32/u32 存放，
            // 只在交给设备时转成 I24/U24。
            cpal::SampleFormat::I24 => self.build_pipeline::<i32, cpal::I24>(
                &config,
                meta,
                should_predecode,
                &mut decode_thread,
                &session,
            )?,
            cpal::SampleFormat::U24 => self.build_pipeline::<u32, cpal::U24>(
                &config,
                meta,
                should_predecode,
                &mut decode_thread,
                &session,
            )?,
            cpal::SampleFormat::I32 => self.build_pipeline::<i32, i32>(
                &config,
                meta,
                should_predecode,
                &mut decode_thread,
                &session,
            )?,
            cpal::SampleFormat::U32 => self.build_pipeline::<u32, u32>(
                &config,
                meta,
                should_predecode,
                &mut decode_thread,
                &session,
            )?,
            cpal::SampleFormat::F32 => self.build_pipeline::<f32, f32>(
                &config,
                meta,
                should_predecode,
                &mut decode_thread,
                &session,
            )?,
            cpal::SampleFormat::F64 => self.build_pipeline::<f64, f64>(
                &config,
                meta,
                should_predecode,
                &mut decode_thread,
                &session,
            )?,
            _ => return Err(format!("Unsupported sample format: {:?}", sample_format).into()),
        };

        let safe_stream = SafeStream::new(stream);
        if let Err(err) = safe_stream.play() {
            // 解码线程已经在跑：不终结会话，它会一直往一个没人消费的 ring buffer
            // 里填样本，直到进程结束。
            session.terminate();
            if let Some(thread) = decode_thread.take() {
                let _ = thread.join();
            }
            return Err(err.into());
        }
        self.stream = Some(safe_stream);
        self.session = Some(session);
        self.decode_thread = decode_thread;
        #[cfg(target_os = "linux")]
        {
            self.device_reservation = device_reservation;
        }
        Ok(())
    }

    /// 建一条完整的播放流水线：ring buffer + 开流前的预解码 + 输出回调 + 解码线程。
    ///
    /// 除样本类型之外，所有格式走的是**同一条**路径——旧实现为 11 种格式各抄
    /// 一遍，任何一次改动（比如改了预解码策略）都得记得改 11 处。
    fn build_pipeline<S, Out>(
        &self,
        config: &cpal::StreamConfig,
        meta: AudioMetadata,
        should_predecode: bool,
        decode_thread_slot: &mut Option<std::thread::JoinHandle<()>>,
        session: &Arc<Session>,
    ) -> Result<cpal::Stream, Box<dyn std::error::Error>>
    where
        S: ConvertibleSample + Copy + Send + 'static,
        Out: cpal::SizedSample + cpal::FromSample<S> + Send + 'static,
    {
        let mut meta = meta;
        let sr = meta.sample_rate;
        let channels = meta.channels;
        let rb = HeapRb::<S>::new(output_buffer_samples(sr, channels));
        let (mut producer, consumer) = rb.split();

        Self::predecode_initial::<S, _>(&mut meta, &mut producer, should_predecode, session);
        let stream = output::build_stream_converted::<S, Out, _>(
            &self.device,
            config,
            consumer,
            Arc::clone(session),
            channels as usize,
        )
        .map_err(|error| stage_error("创建音频输出流", error.as_ref()))?;
        let decode_thread = Self::start_decode_thread::<S>(meta, producer, Arc::clone(session));
        *decode_thread_slot = Some(decode_thread);
        Ok(stream)
    }

    /// 目标设备开不出兼容配置时退回系统默认端点（独占会话禁用此路径）。
    /// 退回后必须同步改写 requested_device_id，否则 output_devices() 会继续把
    /// 用户选的那条标成 is_current——UI 显示的输出端点和实际发声端点对不上。
    fn maybe_fallback_to_default_device(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        if let Some(default) = host.default_output_device()
            && output::device_id(&default) != output::device_id(&self.device)
        {
            println!("[audio] falling back to default device...");
            self.device = default;
            self.requested_device_id = None;
            return Ok(true);
        }
        Ok(false)
    }

    /// 起解码线程。返回句柄：播放器被 drop 时会 join 它，保证线程已经看到
    /// `terminate` 并退出，不会在下一次开播时与旧线程共用同一份状态。
    fn start_decode_thread<S>(
        meta: AudioMetadata,
        producer: impl Producer<Item = S> + Send + 'static,
        session: Arc<Session>,
    ) -> std::thread::JoinHandle<()>
    where
        S: ConvertibleSample + Copy + Send + 'static,
    {
        std::thread::spawn(move || decoder::run_decode_loop::<S, _>(meta, producer, session))
    }

    fn predecode_initial<S, P>(
        meta: &mut AudioMetadata,
        producer: &mut P,
        enabled: bool,
        session: &Session,
    ) where
        S: ConvertibleSample + Copy,
        P: Producer<Item = S>,
    {
        if !enabled {
            return;
        }

        let target_samples =
            predecode_target_samples(meta.sample_rate, meta.channels).min(producer.vacant_len());
        while producer.occupied_len() < target_samples && !producer.is_full() {
            if !decoder::decode_next_packet::<S, _>(
                &mut *meta.format_reader,
                &mut *meta.decoder,
                meta.track_id,
                meta.sample_rate,
                meta.time_base,
                producer,
                session,
            ) {
                // 开播预解码就到 EOF：会话已经没有样本可产出。
                session.mark_drained();
                break;
            }
        }
    }

    /// 没有会话时（未开播 / 已 stop）这些方法一律是安全的空操作。
    /// 等解码线程退出。`terminate` 之后它最多再睡一个 10ms 的轮询间隔；
    /// join 在这里是有限等待，不会卡住控制面。
    fn join_decode_thread(&mut self) {
        if let Some(handle) = self.decode_thread.take() {
            let _ = handle.join();
        }
    }

    pub fn pause(&self) {
        if let Some(session) = self.session.as_ref() {
            session.set_paused(true);
        }
    }

    /// 设置软件音量（0.0..=1.0）：写开播种子 + 应用到当前会话。
    /// BitPerfect 会话内不生效（会话锁定 1.0 直通），种子照常更新。
    /// 非有限值的净化在 `Session::set_volume`（所有音量写入的汇聚点）完成，
    /// 种子沿用同一份净化保证下一位存储干净。
    pub fn set_volume(&mut self, volume: f32) {
        let sanitized = crate::audio::session::sanitize_volume(volume);
        self.volume_seed = sanitized;
        if let Some(session) = self.session.as_ref() {
            session.set_volume(sanitized);
        }
    }

    pub fn resume(&self) {
        if let Some(session) = self.session.as_ref() {
            session.set_paused(false);
        }
    }

    pub fn progress(&self) -> Duration {
        self.session
            .as_ref()
            .map(|session| session.position())
            .unwrap_or(Duration::ZERO)
    }

    pub fn seek(&self, target: Duration) {
        if let Some(session) = self.session.as_ref() {
            session.schedule_seek(target);
        }
    }

    /// 停掉输出并终结当前会话。此后这个播放器**没有会话**：读进度得到 0、
    /// 读终局得到 None。开播失败也走这条路径，所以失败不会留下一个"半死"
    /// 的会话让上层的快照显示上一次的位置。
    pub fn stop(&mut self) {
        if let Some(session) = self.session.take() {
            session.terminate();
        }
        self.join_decode_thread();
        self.stream = None;
        #[cfg(target_os = "linux")]
        {
            self.device_reservation = None;
        }
    }

    /// 会话的终局：`None` 表示仍在进行。
    pub(crate) fn session_outcome(&self) -> Option<SessionOutcome> {
        self.session.as_ref().and_then(|session| session.outcome())
    }

    pub fn session_volume(&self) -> f32 {
        self.session
            .as_ref()
            .map(|session| session.volume())
            .unwrap_or(1.0)
    }

    pub fn session_volume_locked(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.volume_locked())
    }

    pub fn is_buffering(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.buffering())
    }
}

fn output_buffer_samples(sample_rate: u32, channels: u16) -> usize {
    sample_rate as usize * channels as usize * OUTPUT_BUFFER_SECONDS
}

fn predecode_target_samples(sample_rate: u32, channels: u16) -> usize {
    sample_rate as usize * channels as usize * INITIAL_PREDECODE_SECONDS
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            session.terminate();
        }
        // 先让解码线程看到 terminate 并退出，再放掉设备：反过来会让旧线程在
        // 设备已经关闭之后还尝试写 ring buffer。
        self.join_decode_thread();
        #[cfg(target_os = "linux")]
        {
            self.device_reservation = None;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn safe_stream_drop_swallows_panic_and_does_not_propagate() {
        struct PanickingDrop;
        impl Drop for PanickingDrop {
            fn drop(&mut self) {
                panic!("simulated cpal assertion left == right failed (left: -1, right: 8)");
            }
        }

        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut obj = Some(PanickingDrop);
            if let Some(inner) = obj.take() {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    drop(inner);
                }));
            }
        }));

        assert!(
            caught.is_ok(),
            "Panic in drop must be swallowed by catch_unwind"
        );
    }
}
