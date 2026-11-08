//! `(代际, 值)` 打包进一个 u64。
//!
//! 跨线程共享的"哪一代 + 位置"必须是**一次**原子读：分成两个原子量读，读端可能
//! 把新代际与旧值拼在一起，据此算出的进度会跳到另一首歌的位置（或凭空超前）。
//! 打包后不存在这种撕裂组合。
//!
//! 位宽：低 40 位是值（48kHz 下约 282 天或 1678 万次 seek 之后才回绕），
//! 高 24 位是代际。超出范围按上限截断——宁可停在末尾，也不回绕到开头。

pub(crate) const VALUE_BITS: u32 = 40;
pub(crate) const VALUE_MASK: u64 = (1u64 << VALUE_BITS) - 1;
pub(crate) const EPOCH_MASK: u64 = (1u64 << (64 - VALUE_BITS)) - 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EpochWord {
    pub(crate) epoch: u64,
    pub(crate) value: u64,
}

impl EpochWord {
    pub(crate) fn new(epoch: u64, value: u64) -> Self {
        Self {
            epoch: epoch & EPOCH_MASK,
            value: value.min(VALUE_MASK),
        }
    }

    pub(crate) fn pack(self) -> u64 {
        ((self.epoch & EPOCH_MASK) << VALUE_BITS) | self.value
    }

    pub(crate) fn unpack(raw: u64) -> Self {
        Self {
            epoch: raw >> VALUE_BITS,
            value: raw & VALUE_MASK,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_unpacks_without_losing_either_half() {
        for (epoch, value) in [
            (0, 0),
            (1, 48_000),
            (EPOCH_MASK, VALUE_MASK),
            (7, 1_234_567_890),
        ] {
            let word = EpochWord::new(epoch, value);
            assert_eq!(EpochWord::unpack(word.pack()), word);
        }
    }

    #[test]
    fn epoch_and_value_never_bleed_into_each_other() {
        let high_value = EpochWord::new(3, VALUE_MASK);
        let low_value = EpochWord::new(3, 0);
        assert!(high_value.pack() > low_value.pack());
        assert_eq!(EpochWord::unpack(low_value.pack()).epoch, 3);
        assert_eq!(EpochWord::unpack(high_value.pack()).epoch, 3);

        let next_epoch_low_value = EpochWord::new(4, 0);
        assert!(
            next_epoch_low_value.pack() > high_value.pack(),
            "epoch 必须压过值域，否则代际比较会被大帧号吞掉"
        );
    }

    #[test]
    fn clamps_out_of_range_values_instead_of_wrapping() {
        let word = EpochWord::new(2, u64::MAX);
        assert_eq!(word.value, VALUE_MASK);
        assert_eq!(EpochWord::unpack(word.pack()).epoch, 2);
    }
}
