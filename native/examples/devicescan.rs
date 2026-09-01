//! 输出设备诊断：用真实播放链路列举设备、并按指定设备/模式试播一段音频。
//!
//! 列举：       cargo run --example devicescan
//! 试播：       cargo run --example devicescan -- <file> [device-id] [normal|exclusive|bitperfect] [secs] [tracks]
//!
//! 例：cargo run --example devicescan -- ~/Music/a.flac hw:CARD=1,DEV=0 bitperfect 5
//! tracks > 1 时在同一个播放器上连播多次，用来观察切歌是否复用设备预留
//! （独占模式下 pw-reserve 的 PID 应当全程不变）。

use native::audio::AudioPlayer;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let Some(path) = args.first() else {
        list_devices();
        return;
    };

    let device_id = args.get(1).map(String::as_str).filter(|id| *id != "-");
    let mode = args.get(2).map(String::as_str).unwrap_or("normal");
    let secs: u64 = args
        .get(3)
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    let tracks: u32 = args
        .get(4)
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);

    let (strict_bit_perfect, exclusive) = match mode {
        "bitperfect" => (true, true),
        "exclusive" => (false, true),
        "normal" => (false, false),
        other => {
            eprintln!("未知模式 {other}（可选 normal / exclusive / bitperfect）");
            std::process::exit(2);
        }
    };

    println!(
        "试播 {path}\n  设备 = {}\n  模式 = {mode} (strict_bit_perfect={strict_bit_perfect}, exclusive={exclusive})",
        device_id.unwrap_or("<系统默认>")
    );

    let mut player = match AudioPlayer::new(device_id) {
        Ok(player) => player,
        Err(err) => {
            eprintln!("创建播放器失败: {err}");
            std::process::exit(1);
        }
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    for track in 1..=tracks {
        println!("--- 第 {track}/{tracks} 首 ---");
        if let Err(err) =
            runtime.block_on(player.play_file(path, None, strict_bit_perfect, exclusive))
        {
            eprintln!("播放失败: {err}");
            std::process::exit(1);
        }

        for _ in 0..secs {
            std::thread::sleep(Duration::from_secs(1));
            println!("  进度 {:?}", player.progress());
        }
    }

    player.stop();
    println!("完成");
}

fn list_devices() {
    let player = AudioPlayer::new(None).expect("create player");
    let devices = player.output_devices().expect("output devices");

    println!(
        "{} 个输出端点（与设置页看到的是同一份列表）：",
        devices.len()
    );
    for device in devices {
        let mut marks = Vec::new();
        if device.is_default {
            marks.push("default");
        }
        if device.is_current {
            marks.push("current");
        }
        println!(
            "  id={:<24} name={:<40} {}",
            device.id,
            device.name,
            marks.join(",")
        );
    }
}
