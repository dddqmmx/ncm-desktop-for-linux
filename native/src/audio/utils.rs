pub fn estimate_prefetch_bytes(
    content_length: Option<u64>,
    duration_ms: Option<u64>,
    cache_ahead_secs: u32,
) -> u64 {
    let ahead_secs = u64::from(cache_ahead_secs.clamp(5, 300));
    let min_prefetch = 256 * 1024;
    let max_prefetch = 32 * 1024 * 1024;

    let estimated = match (content_length, duration_ms.filter(|duration| *duration > 0)) {
        (Some(content_length), Some(duration_ms)) => {
            let bytes_per_second =
                ((content_length as u128) * 1000 / u128::from(duration_ms)).max(1) as u64;
            bytes_per_second.saturating_mul(ahead_secs)
        }
        _ => ahead_secs.saturating_mul(256 * 1024),
    };

    estimated.clamp(min_prefetch, max_prefetch)
}
