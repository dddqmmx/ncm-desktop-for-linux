//! 输出层：cpal 设备/格式/流。
//!
//! 三个子模块各管一段，彼此只共享 `OutputDeviceInfo`：
//! - [`devices`]：设备身份与 id 归一（含 Linux 的 /proc/asound 解析）；
//! - [`formats`]：采样率/位深/声道协商；
//! - [`stream`]：输出回调与流错误。
//!
//! 跨线程时序不在这里，在 [`crate::audio::session`]。

mod devices;
mod formats;
mod stream;

#[cfg(test)]
mod tests;

pub(crate) use devices::*;
pub(crate) use formats::*;
#[cfg(test)]
pub(crate) use stream::render_block;
pub(crate) use stream::{OutputLatency, build_stream_converted};

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
