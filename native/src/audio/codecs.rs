//! 解码器注册表与音源参数归一化。
//!
//! Symphonia 0.5 的默认注册表里没有 Opus 解码器，这里在默认表上追加
//! `symphonia_adapter_libopus`：它把 bundled 的 libopus 静态链接进 native 模块，
//! 不依赖系统上是否装了 libopus.so。
//!
//! 注册表之上还负责把各容器给出的 `CodecParameters` 抹平成解码器能直接使用的
//! 形态——播放链路其余部分不需要知道“这个 Opus 来自 Ogg、WebM 还是 MP4”。

use std::sync::LazyLock;

use symphonia::core::audio::Channels;
use symphonia::core::codecs::{CODEC_TYPE_OPUS, CodecParameters, CodecRegistry, Decoder};
use symphonia_adapter_libopus::OpusDecoder;

/// Opus 头部签名。Ogg/WebM 直接存这个结构，Symphonia 的 MP4 demuxer 也会补上它。
const OPUS_HEAD_MAGIC: &[u8] = b"OpusHead";

/// libopus 单流解码器只解 1/2 声道；更多声道要走 multistream，适配器尚未提供。
const OPUS_MAX_CHANNELS: u8 = 2;

/// libopus 直接支持的输出采样率（RFC 6716 的固定集合）。Ogg 的 mapper 一律报
/// 48kHz；其它容器报的是容器字段，可能被写成 44100 这类编码器根本不接受的值。
const OPUS_SAMPLE_RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];

/// 兜底采样率：Opus 的原生输出率，也是 Ogg 路径一直使用的值。
const OPUS_DEFAULT_SAMPLE_RATE: u32 = 48_000;

static REGISTRY: LazyLock<CodecRegistry> = LazyLock::new(|| {
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    registry.register_all::<OpusDecoder>();
    registry
});

/// 项目实际使用的解码器注册表：Symphonia 默认表 + Opus。
pub(crate) fn registry() -> &'static CodecRegistry {
    &REGISTRY
}

/// 归一化 track 参数并建好解码器，返回两者。
///
/// 调用方应当把返回的参数（而不是容器原始参数）当作元数据的事实来源：
/// Opus 的声道数可能只写在头部里，位深则被清空。
pub fn prepare_track(
    params: &CodecParameters,
) -> Result<(CodecParameters, Box<dyn Decoder>), Box<dyn std::error::Error + Send + Sync>> {
    let params = normalize(params)?;
    let decoder = registry().make(&params, &Default::default())?;
    Ok((params, decoder))
}

fn normalize(
    params: &CodecParameters,
) -> Result<CodecParameters, Box<dyn std::error::Error + Send + Sync>> {
    if params.codec == CODEC_TYPE_OPUS {
        normalize_opus(params)
    } else {
        Ok(params.clone())
    }
}

fn normalize_opus(
    params: &CodecParameters,
) -> Result<CodecParameters, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = params.clone();

    // Opus 解码输出恒为 f32 的有损 PCM，容器里报的“位深”对它没有意义：WebM 会带
    // 一个 bit_depth=16 / S16，照着它协商输出会把有损音源当成可逐位直通的 PCM，
    // BitPerfect 校验因此形同虚设。清空后走普通协商，输出默认取 f32。
    params.bits_per_sample = None;
    params.sample_format = None;

    let head_channels = match params.extra_data.as_deref_mut() {
        Some(extra) => rewrite_opus_head(extra),
        None => None,
    };

    // 头部是声道数的唯一可靠来源（MP4 的 demuxer 完全不填声道）；没有头部时才
    // 退回容器参数。
    let channels = head_channels
        .map(|channels| channels as u16)
        .or_else(|| params.channels.map(|channels| channels.count() as u16))
        .filter(|channels| *channels > 0)
        .ok_or("Opus 音源缺少声道信息（OpusHead 缺失或过短）")?;

    // 容器报的采样率是“编码前的输入采样率”，不一定是 libopus 能开的输出率
    // （它只接受 8/12/16/24/48k）。落到不支持的值时统一按 48k 输出——Opus 解码
    // 本来就在解码器内重采样，Ogg 路径一直如此。
    params.sample_rate = Some(
        params
            .sample_rate
            .filter(|rate| OPUS_SAMPLE_RATES.contains(rate))
            .unwrap_or(OPUS_DEFAULT_SAMPLE_RATE),
    );

    if channels > u16::from(OPUS_MAX_CHANNELS) {
        return Err(
            format!("Opus 多声道（{channels} 声道）暂不支持：解码器只解单声道与立体声").into(),
        );
    }

    let channels_mask = match channels {
        1 => Channels::FRONT_LEFT,
        _ => Channels::FRONT_LEFT | Channels::FRONT_RIGHT,
    };

    // 适配器的声道数优先取 channel_layout；容器标错的布局会让它盖掉头部里的
    // 正确值，不一致时以头部为准丢掉布局。
    if let Some(layout) = params.channel_layout
        && layout.into_channels().count() as u16 != channels
    {
        params.channel_layout = None;
    }
    params.channels = Some(channels_mask);

    Ok(params)
}

/// 把 Opus 头部就地归一成 Ogg 形态，返回头部声明的声道数。
///
/// 两种容器给出的头部字节序不同：
/// - Ogg / WebM：RFC 7845 的 `OpusHead`，version=1，pre-skip / 输入采样率 / 增益
///   都是小端；
/// - MP4：`dOps` box 的裸内容（Symphonia 的 isomp4 只在前面补了 `OpusHead`
///   签名），version=0，同样三个字段是大端。
///
/// libopus 适配器按小端读 pre-skip，直接喂 dOps 形态会把 312 读成 14337：
/// 首包会被整包裁掉，开头 20ms 音频凭空消失。这里按 version 判断来源并换回
/// 小端，同时把 version 改成 1，让重复归一化不会来回翻转字节序。
fn rewrite_opus_head(extra: &mut [u8]) -> Option<u8> {
    if extra.len() < 10 || &extra[..8] != OPUS_HEAD_MAGIC {
        return None;
    }

    if extra[8] == 0 {
        // dOps：pre-skip(u16) / 输入采样率(u32) / 增益(i16)，逐个换成小端。
        for (start, len) in [(10, 2), (12, 4), (16, 2)] {
            if let Some(field) = extra.get_mut(start..start + len) {
                field.reverse();
            }
        }
        extra[8] = 1;
    }

    Some(extra[9])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use symphonia::core::formats::{FormatOptions, FormatReader};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    fn fixture(name: &str) -> String {
        format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn open_format(path: &str) -> Box<dyn FormatReader> {
        let file = File::open(path).unwrap();
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        symphonia::default::get_probe()
            .format(
                &Hint::new(),
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .unwrap()
            .format
    }

    fn track_params(path: &str) -> CodecParameters {
        open_format(path).tracks()[0].codec_params.clone()
    }

    /// `Box<dyn Decoder>` 不是 `Debug`，`unwrap_err` 用不了。
    fn error_message<T>(result: Result<T, Box<dyn std::error::Error + Send + Sync>>) -> String {
        match result {
            Ok(_) => panic!("expected an error"),
            Err(err) => err.to_string(),
        }
    }

    fn opus_pre_skip(params: &CodecParameters) -> u16 {
        let extra = params.extra_data.as_deref().expect("opus extra data");
        u16::from_le_bytes([extra[10], extra[11]])
    }

    #[test]
    fn registers_an_opus_decoder_on_the_default_registry() {
        assert!(
            registry().get_codec(CODEC_TYPE_OPUS).is_some(),
            "Opus 必须在注册表里，否则 gapless/seek 之外的整条解码链路都拿不到解码器"
        );
    }

    /// MP4 的 dOps 用大端存 pre-skip，且 demuxer 不填声道。归一化后两者都必须可用，
    /// 否则 pre-skip 会被读成 14337，首包被整包裁掉。
    #[test]
    fn mp4_opus_head_is_rewritten_from_big_endian_dops() {
        let params = track_params(&fixture("opus-stereo.m4a"));
        assert_eq!(params.channels, None, "MP4 demuxer 本就不填声道");
        assert_eq!(
            opus_pre_skip(&params) & 0xff00,
            0x3800,
            "dOps 里 312 以大端出现：0x0138 被小端读成两个字节时是 0x3801"
        );

        let (normalized, _) = prepare_track(&params).unwrap();

        assert_eq!(opus_pre_skip(&normalized), 312);
        assert_eq!(
            normalized.channels.map(|channels| channels.count()),
            Some(2)
        );
        assert_eq!(normalized.extra_data.as_deref().unwrap()[8], 1);

        // 再归一化一次不得把字节序换回去。
        let (again, _) = prepare_track(&normalized).unwrap();
        assert_eq!(again.extra_data, normalized.extra_data);
        assert_eq!(opus_pre_skip(&again), 312);
    }

    #[test]
    fn ogg_opus_head_is_left_byte_for_byte_intact() {
        let params = track_params(&fixture("opus-mono.ogg"));
        assert_eq!(opus_pre_skip(&params), 312);

        let (normalized, _) = prepare_track(&params).unwrap();

        assert_eq!(normalized.extra_data, params.extra_data);
        assert_eq!(opus_pre_skip(&normalized), 312);
        assert_eq!(
            normalized.channels.map(|channels| channels.count()),
            Some(1)
        );
    }

    /// WebM 会给 Opus 标一个 bit_depth（实测 16 或 32，视封装器而定），那是容器
    /// 字段而不是真实输出格式。
    #[test]
    fn webm_opus_source_does_not_claim_a_bit_depth() {
        let params = track_params(&fixture("opus-stereo.webm"));
        assert!(
            params.bits_per_sample.is_some(),
            "fixture 必须带上假位深才有意义"
        );

        let (normalized, _) = prepare_track(&params).unwrap();

        assert_eq!(normalized.bits_per_sample, None);
        assert!(normalized.sample_format.is_none());
        assert_eq!(
            normalized.channels.map(|channels| channels.count()),
            Some(2)
        );
    }

    /// 容器里的采样率是编码前的输入率，不一定是 libopus 能开的输出率。落到 44.1k
    /// 这类值时解码器会直接拒绝建流，必须归一。
    #[test]
    fn unsupported_sample_rates_fall_back_to_the_opus_native_rate() {
        let mut params = track_params(&fixture("opus-stereo.m4a"));

        params.sample_rate = Some(44_100);
        let (normalized, _) = prepare_track(&params).unwrap();
        assert_eq!(normalized.sample_rate, Some(48_000));

        // 合法的输出率必须原样保留。
        for rate in OPUS_SAMPLE_RATES {
            params.sample_rate = Some(rate);
            let (normalized, _) = prepare_track(&params).unwrap();
            assert_eq!(normalized.sample_rate, Some(rate), "{rate}Hz 应当原样保留");
        }
    }

    /// 首包必须还能解出音频：pre-skip 只该裁掉包头的 312 帧（960 -> 648），
    /// 而不是把整个包裁空。
    ///
    /// 包时长按 track 的 timebase 换算成帧——WebM 的 dur 是毫秒，Ogg 是 48kHz 帧，
    /// 直接拿 dur 当帧数会得出错误的期望值。
    #[test]
    fn first_packet_survives_the_pre_skip_trim_in_every_container() {
        const OPUS_RATE: u64 = 48_000;

        for name in ["opus-mono.ogg", "opus-stereo.webm", "opus-stereo.m4a"] {
            let path = fixture(name);
            let params = track_params(&path);
            let time_base = params
                .time_base
                .expect("timebase is needed to size the packet");
            let (_, mut decoder) = prepare_track(&params).unwrap();
            let mut format = open_format(&path);

            let packet = format.next_packet().unwrap();
            let packet_frames =
                (u128::from(packet.dur()) * u128::from(time_base.numer) * u128::from(OPUS_RATE)
                    / u128::from(time_base.denom)) as usize;
            let decoded = decoder.decode(&packet).unwrap();

            assert!(
                decoded.frames() > 0,
                "{name}: pre-skip 处理错误会把首包整包裁空，开头 20ms 音频消失"
            );
            assert!(
                decoded.frames() < packet_frames,
                "{name}: pre-skip 本应裁掉包头的 312 帧（包 {packet_frames} 帧）"
            );
        }
    }

    /// 本地曲库的时长来自 `probe_file_duration_ms`。各容器对 Opus 的 n_frames / dur
    /// 语义都不同（Ogg 用 48kHz 帧、WebM 用毫秒），这里把三条路径钉死。
    ///
    /// 容差 30ms：Symphonia 在 gapless 关闭时把编码器尾部 padding 算进 n_frames，
    /// 所有格式（MP3/Opus…）都是这样，不是 Opus 特有问题。
    #[test]
    fn file_duration_matches_the_fixture_for_every_container() {
        for (name, expected_ms) in [
            ("opus-mono.ogg", 2006),
            ("opus-stereo.webm", 2000),
            ("opus-stereo.m4a", 2000),
        ] {
            let ms = crate::audio::decoder::probe_file_duration_ms_blocking(&fixture(name))
                .unwrap_or_else(|err| panic!("{name}: {err}"));
            let delta = ms.abs_diff(expected_ms);

            assert!(
                delta <= 30,
                "{name}: duration {ms}ms, expected ~{expected_ms}ms"
            );
        }
    }

    #[test]
    fn multichannel_opus_is_rejected_with_a_readable_error() {
        let mut params = CodecParameters::new();
        let mut head = Vec::from(OPUS_HEAD_MAGIC);
        head.extend_from_slice(&[1, 6, 0x38, 0x01, 0x80, 0xbb, 0x00, 0x00, 0x00, 0x00, 0x00]);
        params
            .for_codec(CODEC_TYPE_OPUS)
            .with_sample_rate(48_000)
            .with_extra_data(head.into_boxed_slice());

        let err = error_message(prepare_track(&params));

        assert!(err.contains("多声道"), "错误必须说清是声道数问题：{err}");
    }

    #[test]
    fn missing_opus_head_is_reported_instead_of_failing_inside_the_decoder() {
        let mut params = CodecParameters::new();
        params.for_codec(CODEC_TYPE_OPUS).with_sample_rate(48_000);

        let err = error_message(prepare_track(&params));

        assert!(err.contains("声道信息"), "错误必须指向缺失的头部：{err}");
    }
}
