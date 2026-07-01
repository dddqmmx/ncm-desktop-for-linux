use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use symphonia::core::codecs::CODEC_TYPE_NULL;
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

// 复刻 SeekableSource：底层是部分下载的缓存文件，但 byte_len 上报完整 content_length。
struct Partial {
    f: File,
    reported: u64,
}
impl Read for Partial {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.f.read(b) // 真实文件到结尾就返回 Ok(0)（EOF），模拟未下载部分读不到
    }
}
impl Seek for Partial {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.f.seek(p)
    }
}
impl MediaSource for Partial {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.reported)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let reported: u64 = args[2].parse().unwrap();
    let f = File::open(path).unwrap();
    let real = f.metadata().unwrap().len();
    println!(
        "file={} real_len={} reported(content_length)={}  downloaded={:.1}%",
        path,
        real,
        reported,
        real as f64 / reported as f64 * 100.0
    );
    let src = Partial { f, reported };
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
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
    let (tid, sr, nframes) = {
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .unwrap();
        (
            track.id,
            track.codec_params.sample_rate.unwrap_or(44100) as f64,
            track.codec_params.n_frames,
        )
    };
    println!(
        "track sr={} n_frames={:?} full_duration={:.1}s  (cached audio ~first {:.0}s)",
        sr,
        nframes,
        nframes.map(|n| n as f64 / sr).unwrap_or(0.0),
        nframes.map(|n| n as f64 / sr).unwrap_or(0.0) * (real as f64 / reported as f64)
    );

    for &t in &[5.0_f64, 15.0, 25.0, 35.0, 45.0, 60.0, 120.0] {
        match format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: Time::from(t),
                track_id: Some(tid),
            },
        ) {
            Ok(seeked) => {
                let req = seeked.required_ts as f64 / sr;
                let act = seeked.actual_ts as f64 / sr;
                match format.next_packet() {
                    Ok(p) => {
                        let pkt = p.ts() as f64 / sr;
                        println!(
                            "  seek {:>5.0}s -> required={:.2}s actual={:.2}s nextPkt={:.2}s  | actual-target={:+.2}s pkt-target={:+.2}s",
                            t, req, act, pkt, act - t, pkt - t
                        );
                    }
                    Err(e) => println!(
                        "  seek {:>5.0}s -> required={:.2}s actual={:.2}s  nextPkt=ERR({:?})  [数据未下载/EOF]",
                        t, req, act, e
                    ),
                }
            }
            Err(e) => println!("  seek {:>5.0}s -> SEEK ERR {:?}", t, e),
        }
    }
}
