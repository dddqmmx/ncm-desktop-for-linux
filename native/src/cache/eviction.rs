use crate::cache::types::{CacheEntry, now_unix_secs};

pub struct EvictionPlanner;

impl EvictionPlanner {
    pub fn select_victims<'a>(
        entries: impl Iterator<Item = (&'a String, &'a CacheEntry)>,
        current_size_bytes: u64,
        max_size_bytes: u64,
    ) -> Vec<String> {
        if max_size_bytes == 0 || current_size_bytes <= max_size_bytes {
            return collect_expired_victims(entries);
        }

        let now = now_unix_secs();
        let mut candidates = entries
            .map(|(key, entry)| {
                let is_expired = entry.expires_at.map(|t| now >= t).unwrap_or(false);
                (
                    key.clone(),
                    entry.accessed_at,
                    entry.created_at,
                    entry.size_bytes,
                    is_expired,
                )
            })
            .collect::<Vec<_>>();

        candidates.sort_by(|a, b| {
            b.4
                .cmp(&a.4)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        let mut remaining = current_size_bytes;
        let mut victims = Vec::new();

        for (key, _, _, size_bytes, _) in candidates {
            if remaining <= max_size_bytes {
                break;
            }

            victims.push(key);
            remaining = remaining.saturating_sub(size_bytes);
        }

        victims
    }
}

fn collect_expired_victims<'a>(
    entries: impl Iterator<Item = (&'a String, &'a CacheEntry)>,
) -> Vec<String> {
    let now = now_unix_secs();
    entries
        .filter(|(_, entry)| entry.expires_at.map(|t| now >= t).unwrap_or(false))
        .map(|(key, _)| key.clone())
        .collect()
}
