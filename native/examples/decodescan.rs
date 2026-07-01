use std::fs::File;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
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
    hint.with_extension("flac");
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .expect("probe failed");
    let mut format = probed.format;
    let (tid, sr, ch, bps, n_frames) = {
        let t = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .unwrap();
        (
            t.id,
            t.codec_params.sample_rate.unwrap_or(0),
            t.codec_params.channels.map(|c| c.count()).unwrap_or(0),
            t.codec_params.bits_per_sample.unwrap_or(0),
            t.codec_params.n_frames,
        )
    };
    let mut decoder = {
        let t = format.tracks().iter().find(|t| t.id == tid).unwrap();
        symphonia::default::get_codecs()
            .make(&t.codec_params, &DecoderOptions::default())
            .unwrap()
    };
    println!(
        "file_len={} sr={} ch={} bits={} declared_frames={:?} declared_dur={:.2}s",
        real_len,
        sr,
        ch,
        bps,
        n_frames,
        n_frames.map(|n| n as f64 / sr as f64).unwrap_or(0.0)
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
    let declared = n_frames.unwrap_or(0);
    println!("packets={} decoded_frames={} decoded_dur={:.2}s last_end_ts={}", packets, total_frames, decoded_dur, last_end);
    println!("decode_errors={} last_err={:?}", decode_errors, last_err);
    if let Some(n) = n_frames {
        let diff = total_frames as i64 - n as i64;
        println!(
            "declared_frames={} decoded_frames={} diff={} ({:+.3}s)  => {}",
            n,
            total_frames,
            diff,
            diff as f64 / sr as f64,
            if diff.abs() < sr as i64 / 10 && decode_errors == 0 {
                "INTACT (decodes fully, duration matches)"
            } else {
                "MISMATCH / possibly corrupt-or-truncated"
            }
        );
    }
    let _ = declared;
}
