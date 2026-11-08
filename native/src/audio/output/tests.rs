//! 输出层测试：全部是纯计算（设备 id 归一、格式选择、缓冲大小），
//! 不依赖真实声卡。流回调的时序由 `audio::session::tests` 覆盖。

#[cfg(test)]
mod output_tests {
    use crate::audio::output::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use symphonia::core::sample::SampleFormat as SymphoniaSampleFormat;

    // cpal 的设备枚举/开流接口无法在无真实声卡的测试环境里构造，因此这里不去
    // 碰 `build_stream*`，只测后端里纯计算的决策（格式选择、设备 id 归一）。
    // 回调的字节级行为由 `session::tests` 通过同一个 `Session` 接口覆盖。

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

    // ------------------------------------------------------------ 输出回调

    mod render {
        use crate::audio::output::{OutputLatency, render_block};
        use crate::audio::session::{OutputCursor, Session};
        use ringbuf::HeapRb;
        use ringbuf::traits::{Observer, Producer, Split};
        use std::sync::Arc;
        use std::time::Duration;

        const SR: u32 = 48_000;
        const CH: usize = 2;

        fn session(filled: bool) -> (Arc<Session>, Vec<i16>) {
            let session = Arc::new(Session::new(SR, CH as u16, 1.0, false, Duration::ZERO));
            let threshold = SR as usize * CH;
            let samples = if filled {
                let samples = vec![1_000i16; threshold + 4_800];
                assert!(session.allow_output(samples.len()));
                samples
            } else {
                assert!(!session.allow_output(0));
                Vec::new()
            };
            (session, samples)
        }

        fn run(session: &Arc<Session>, samples: &[i16], block_frames: usize) -> (Vec<i16>, usize) {
            let rb = HeapRb::<i16>::new(SR as usize * CH * 2);
            let (mut producer, mut consumer) = rb.split();
            producer.push_slice(samples);

            let mut data = vec![0i16; block_frames * CH];
            let mut cursor = OutputCursor::attach(session);
            render_block::<i16, i16, _>(
                &mut consumer,
                &mut data,
                session,
                &mut cursor,
                CH,
                OutputLatency::reported(Duration::ZERO),
            );
            (data, consumer.occupied_len())
        }

        #[test]
        fn a_full_buffer_is_delivered_and_counted_as_frames() {
            let (session, samples) = session(true);
            let block_frames = 960;
            let (data, remaining) = run(&session, &samples, block_frames);

            assert_eq!(remaining, samples.len() - block_frames * CH);
            assert!(data.iter().all(|sample| *sample == 1_000));
            assert_eq!(
                session.position_frames_at(std::time::Instant::now() + Duration::from_millis(100)),
                block_frames as u64
            );
        }

        /// 静音分支不消费样本：否则缓冲会被"听起来没在播"的回调偷偷放空，
        /// 恢复出声时位置与听感错位。
        #[test]
        fn a_muted_block_consumes_nothing() {
            let (session, _) = session(false);
            let samples = vec![1_000i16; 4_800];
            let (data, remaining) = run(&session, &samples, 480);

            assert_eq!(remaining, samples.len());
            assert!(data.iter().all(|sample| *sample == 0));
        }

        #[test]
        fn a_short_buffer_is_padded_with_silence_and_the_rest_is_reported_as_starved() {
            let (session, mut samples) = session(true);
            samples.truncate(100 * CH);
            let block_frames = 960;

            let (data, remaining) = run(&session, &samples, block_frames);

            assert_eq!(remaining, 0);
            assert!(data[..100 * CH].iter().all(|sample| *sample == 1_000));
            assert!(
                data[100 * CH..].iter().all(|sample| *sample == 0),
                "不足的部分补静音，而不是重复最后一段样本"
            );
        }

        #[test]
        fn volume_below_unity_scales_the_samples() {
            let (session, samples) = session(true);
            session.set_volume(0.5);
            let (data, _) = run(&session, &samples, 480);

            assert!(data.iter().all(|sample| *sample == 500));
        }

        #[test]
        fn bit_perfect_session_passes_samples_through_untouched() {
            let session = Arc::new(Session::new(SR, CH as u16, 1.0, true, Duration::ZERO));
            let samples = vec![i16::MAX; SR as usize * CH + 4_800];
            assert!(session.allow_output(samples.len()));
            session.set_volume(0.1);

            let (data, _) = run(&session, &samples, 480);

            assert!(
                data.iter().all(|sample| *sample == i16::MAX),
                "BitPerfect 会话必须逐位直通"
            );
        }

        /// 解码完成 + 缓冲放空 = 交付完毕；回调本身不判断"结束了"，
        /// 只把"我空了"报给会话。
        #[test]
        fn starving_after_the_decoder_drained_ends_the_session() {
            let (session, samples) = session(true);
            session.mark_drained();
            run(&session, &samples, samples.len() / CH);

            let (data, _) = run(&session, &[], 480);

            assert!(data.iter().all(|sample| *sample == 0));
            assert_eq!(
                session.outcome(),
                Some(crate::audio::session::SessionOutcome::Delivered)
            );
        }
    }
}
