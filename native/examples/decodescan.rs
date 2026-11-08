use std::fs::File;
use symphonia::core::codecs::CODEC_TYPE_NULL;
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let f = File::open(&path).unwrap();
    let real_len = f.metadata().unwrap().len();
    let mss = MediaSourceStream::new(Box::new(f), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = std::path::Path::new(&path)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        hint.with_extension(extension);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .expect("probe failed");
    let mut format = probed.format;
    // 走真实播放链路的参数归一化 + 解码器注册表（含 Opus）。
    let (tid, sr, ch, bps, n_frames, mut decoder) = {
        let t = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .unwrap();
        let (params, decoder) = native::audio::codecs::prepare_track(&t.codec_params).unwrap();
        (
            t.id,
            params.sample_rate.unwrap_or(0),
            params.channels.map(|c| c.count()).unwrap_or(0),
            params.bits_per_sample.unwrap_or(0),
            params.n_frames,
            decoder,
        )
    };
    // n_frames 的时间单位是 track 的 timebase，不是采样帧：WebM 下它是毫秒。
    let time_base = format.tracks()[0]
        .codec_params
        .time_base
        .unwrap_or(symphonia::core::units::TimeBase::new(1, sr.max(1)));
    println!(
        "file_len={} sr={} ch={} bits={} n_frames={:?} declared_dur={:.2}s",
        real_len,
        sr,
        ch,
        bps,
        n_frames,
        n_frames
            .map(|n| time_base.calc_time(n))
            .map(|time| time.seconds as f64 + time.frac)
            .unwrap_or(0.0)
    );

    let mut total_frames: u64 = 0;
    let mut packets: u64 = 0;
    let mut decode_errors: u64 = 0;
    let mut last_err: Option<String> = None;
    let mut first_ts: Option<u64> = None;
    let mut last_end: u64 = 0;
    loop {
        match format.next_packet() {
            Ok(p) => {
                if p.track_id() != tid {
                    continue;
                }
                if first_ts.is_none() {
                    first_ts = Some(p.ts());
                }
                match decoder.decode(&p) {
                    Ok(buf) => {
                        let fr = buf.frames() as u64;
                        total_frames += fr;
                        last_end = p.ts() + fr;
                    }
                    Err(Error::DecodeError(e)) => {
                        decode_errors += 1;
                        last_err = Some(format!("DecodeError: {e}"));
                    }
                    Err(e) => {
                        last_err = Some(format!("FatalDecode: {e}"));
                        break;
                    }
                }
                packets += 1;
            }
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                last_err = Some(format!("next_packet: {e}"));
                break;
            }
        }
    }

    let decoded_dur = total_frames as f64 / sr as f64;
    println!(
        "packets={} decoded_frames={} decoded_dur={:.2}s last_end_ts={}",
        packets, total_frames, decoded_dur, last_end
    );
    println!("decode_errors={} last_err={:?}", decode_errors, last_err);
    // n_frames 以 track timebase 为单位（WebM 下是毫秒），必须先换算成秒再比较。
    if let Some(n) = n_frames {
        let time = time_base.calc_time(n);
        let declared_frames = (time.seconds as f64 + time.frac) * f64::from(sr);
        let diff = total_frames as f64 - declared_frames;
        println!(
            "declared_frames={:.0} decoded_frames={} diff={:+.0} ({:+.3}s)  => {}",
            declared_frames,
            total_frames,
            diff,
            diff / f64::from(sr),
            if diff.abs() < f64::from(sr) / 10.0 && decode_errors == 0 {
                "INTACT (decodes fully, duration matches)"
            } else {
                "MISMATCH / possibly corrupt-or-truncated"
            }
        );
    }
}
