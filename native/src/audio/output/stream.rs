//! cpal 输出流：回调本体与流级错误处理。
//!
//! 回调只做三件事：把样本交给设备、把位置写回 [`Session`]、把"我空了"报告给
//! 会话。所有判定（要不要出声、seek 换代、播完了没有）都在会话里，回调里没有
//! 跨回调状态——每次回调新建一个 [`OutputCursor`]。
//!
//! 回调的实体是 [`render_block`]：与 cpal 解耦，测试可以直接喂一块样本进去
//! 断言输出字节与位置变化，不需要真实声卡。

use cpal::FromSample;
use cpal::traits::DeviceTrait;
use ringbuf::traits::{Consumer, Observer};
use std::sync::Arc;
use std::time::Duration;

use crate::audio::session::{OutputBlock, OutputCursor, Session};

/// 输出延迟：从回调时刻到这段样本真正发声的间隔（驱动/硬件报告）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OutputLatency(Duration);

impl OutputLatency {
    #[cfg(test)]
    pub(crate) fn reported(latency: Duration) -> Self {
        Self(latency)
    }

    pub(crate) fn total(self) -> Duration {
        self.0
    }
}

fn output_latency(info: &cpal::OutputCallbackInfo) -> OutputLatency {
    let timestamp = info.timestamp();
    OutputLatency(
        timestamp
            .playback
            .duration_since(&timestamp.callback)
            .unwrap_or(Duration::ZERO),
    )
}

pub(crate) fn build_stream_converted<In, Out, C>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut consumer: C,
    session: Arc<Session>,
    channels: usize,
) -> Result<cpal::Stream, Box<dyn std::error::Error>>
where
    In: Copy + Send + 'static,
    Out: cpal::SizedSample + cpal::FromSample<In> + Send + 'static,
    C: Consumer<Item = In> + Observer<Item = In> + Send + 'static,
{
    let channels = channels.max(1);
    let error_session = Arc::clone(&session);
    let stream = device.build_output_stream(
        config,
        move |data: &mut [Out], info: &cpal::OutputCallbackInfo| {
            // 回调私有的代际游标。换代（seek）、暂停、缓冲不足、会话终局都在
            // `begin` 里一次判定完，回调本身不持有任何跨回调状态。
            let mut cursor = OutputCursor::attach(&session);
            render_block(
                &mut consumer,
                data,
                &session,
                &mut cursor,
                channels,
                output_latency(info),
            );
        },
        move |err| handle_stream_error(err, &error_session),
        None,
    )?;
    Ok(stream)
}

/// 一次输出回调的全部工作，与 cpal 解耦以便测试。
///
/// 契约（测试逐条覆盖）：
/// - 会话判定为静音时整块填静音，**不**消费 ring buffer 的一个样本；
/// - 交付时逐个样本转换 + 应用软件音量（1.0 完全直通），不够的部分补静音；
/// - 交付量按帧记入会话；一个样本都没取到时报告"我空了"让会话判定终局。
pub(crate) fn render_block<In, Out, C>(
    consumer: &mut C,
    data: &mut [Out],
    session: &Session,
    cursor: &mut OutputCursor,
    channels: usize,
    latency: OutputLatency,
) where
    In: Copy,
    Out: cpal::SizedSample + cpal::FromSample<In>,
    C: Consumer<Item = In> + Observer<Item = In>,
{
    let block = cursor.begin(session, consumer.occupied_len(), || {
        // clear() 对 Copy 样本就是一次读指针推进（drop 为 no-op 被编译器消除）；
        // 逐样本 try_pop 在 seek 时要弹掉最多 6 秒的缓冲，每个样本一次原子操作，
        // 是回调超时（xrun）的隐患。
        consumer.clear();
    });

    if block == OutputBlock::Silence {
        data.fill(Out::EQUILIBRIUM);
        return;
    }

    // 软件音量：1.0 时完全直通，不触碰样本（BitPerfect 会话恒为 1.0）。
    let volume = session.volume();
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
        cursor.delivered(session, (samples_read / channels.max(1)) as u64, latency);
    } else {
        // 缓冲已空：由会话判定"交付完毕"还是"重新缓冲"，回调不再自行把
        // is_finished 拉高——终局只能由会话在同一份状态上判定。
        cursor.starved(session);
    }
}

/// 输出流的致命错误处理。设备被拔出/被抢占后，CPAL 的 ALSA worker 可能会在
/// poll 返回 POLLERR 时进入无休止自旋重试循环：会话的 `fail` 幂等，并通过
/// `eprintln!`（内部用 writeln! 并忽略错误）避免 stderr 管道写满时
/// （os error 11 EAGAIN）panic 导致音频线程崩溃。
/// 随后短休眠抑制 100% CPU 自旋，为外层检测会话终局并销毁流留出安全窗口。
/// 设备类错误在会话里标记为 device_lost：worker 据此把会话自动迁移到系统默认
/// 续播，而不是当成普通流错误终止会话（解码器/网络错误不置位，仍走终止路径）。
fn handle_stream_error(err: cpal::StreamError, session: &Session) {
    match err {
        cpal::StreamError::DeviceNotAvailable | cpal::StreamError::StreamInvalidated => {
            session.fail(true, &format!("[audio] output stream lost: {err}"));
            std::thread::sleep(Duration::from_millis(50));
        }
        // 欠载只是一次爆音，ALSA 自己会恢复，结束会话属于过度反应。
        cpal::StreamError::BufferUnderrun => {}
        // 其余 host 特定错误按致命处理：ALSA 上设备拔出/被抢占往往以裸 errno
        // （ENODEV/EINVAL 等）或 poll POLLERR 的 BackendSpecific 出现而非上面的类型化变体。
        cpal::StreamError::BackendSpecific { err } => {
            session.fail(
                true,
                &format!("[audio] output stream fatal error: {}", err.description),
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
