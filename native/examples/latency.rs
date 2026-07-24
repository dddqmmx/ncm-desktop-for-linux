use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

fn main() {
    let host = cpal::default_host();
    let device = host.default_output_device().expect("no default output device");
    println!(
        "device: {:?}",
        device
            .description()
            .map(|desc| desc.name().to_string())
            .unwrap_or_else(|_| "Unknown Device".to_string())
    );

    // 可选参数：目标采样率（复刻 app 的 find_best_config，按源采样率开流）
    let target_sr: Option<u32> = std::env::args().nth(1).and_then(|s| s.parse().ok());

    let (config, sample_format) = if let Some(sr) = target_sr {
        let mut matches = Vec::new();
        for c in device.supported_output_configs().expect("configs") {
            if c.channels() == 2 && sr >= c.min_sample_rate() && sr <= c.max_sample_rate() {
                matches.push(c);
            }
        }
        // 优先 F32，其次 I16/I32，避免选到 U8
        let pick = matches
            .iter()
            .find(|c| c.sample_format() == cpal::SampleFormat::F32)
            .or_else(|| matches.iter().find(|c| c.sample_format() == cpal::SampleFormat::I16))
            .or_else(|| matches.iter().find(|c| c.sample_format() == cpal::SampleFormat::I32))
            .cloned();
        match pick {
            Some(c) => {
                let cfg = c.with_sample_rate(sr);
                println!("forcing target_sr={}", sr);
                (cfg.config(), cfg.sample_format())
            }
            None => {
                println!("target_sr={} NOT supported (2ch, F32/I16/I32)", sr);
                return;
            }
        }
    } else {
        let supported = device.default_output_config().expect("no default config");
        (supported.config(), supported.sample_format())
    };
    println!(
        "stream config: sample_rate={} channels={} buffer_size={:?} format={:?}",
        config.sample_rate, config.channels, config.buffer_size, sample_format
    );

    let lat_us = Arc::new(AtomicU64::new(0));
    let max_us = Arc::new(AtomicU64::new(0));
    let calls = Arc::new(AtomicU32::new(0));
    let first_frames = Arc::new(AtomicU64::new(0));

    let make_cb = |lat: Arc<AtomicU64>, mx: Arc<AtomicU64>, c: Arc<AtomicU32>, ff: Arc<AtomicU64>, chan: usize| {
        move |len: usize, ts: cpal::OutputStreamTimestamp| {
            if let Some(d) = ts.playback.duration_since(&ts.callback) {
                let us = d.as_micros() as u64;
                lat.store(us, Ordering::Relaxed);
                mx.fetch_max(us, Ordering::Relaxed);
            }
            let n = c.fetch_add(1, Ordering::Relaxed);
            if n == 0 {
                ff.store((len / chan) as u64, Ordering::Relaxed);
            }
        }
    };

    let err = |e| eprintln!("stream error: {:?}", e);
    let chan = config.channels as usize;

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let cb = make_cb(lat_us.clone(), max_us.clone(), calls.clone(), first_frames.clone(), chan);
            device
                .build_output_stream(
                    &config,
                    move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                        for s in data.iter_mut() {
                            *s = 0.0;
                        }
                        cb(data.len(), info.timestamp());
                    },
                    err,
                    None,
                )
                .expect("build f32 stream")
        }
        cpal::SampleFormat::I16 => {
            let cb = make_cb(lat_us.clone(), max_us.clone(), calls.clone(), first_frames.clone(), chan);
            device
                .build_output_stream(
                    &config,
                    move |data: &mut [i16], info: &cpal::OutputCallbackInfo| {
                        for s in data.iter_mut() {
                            *s = 0;
                        }
                        cb(data.len(), info.timestamp());
                    },
                    err,
                    None,
                )
                .expect("build i16 stream")
        }
        other => {
            println!("unhandled sample format {:?}, trying f32 anyway", other);
            return;
        }
    };

    stream.play().expect("play");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let sr = config.sample_rate as f64;
    println!("callbacks: {}", calls.load(Ordering::Relaxed));
    println!("first callback frames: {}", first_frames.load(Ordering::Relaxed));
    let last = lat_us.load(Ordering::Relaxed);
    let max = max_us.load(Ordering::Relaxed);
    println!(
        "output latency (playback - callback): last={:.1} ms, max={:.1} ms  (= {:.0} / {:.0} frames @ {}Hz)",
        last as f64 / 1000.0,
        max as f64 / 1000.0,
        last as f64 / 1_000_000.0 * sr,
        max as f64 / 1_000_000.0 * sr,
        sr as u32
    );
}
