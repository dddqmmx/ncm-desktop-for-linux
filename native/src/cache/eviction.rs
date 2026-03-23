use crate::cache::types::CacheEntry;

pub struct EvictionPlanner;

impl EvictionPlanner {
    pub fn select_victims<'a>(
        entries: impl Iterator<Item = (&'a String, &'a CacheEntry)>,
        current_size_bytes: u64,
        max_size_bytes: u64,
    ) -> Vec<String> {
        if max_size_bytes == 0 || current_size_bytes <= max_size_bytes {
            return Vec::new();
        }

        let mut candidates = entries
            .map(|(key, entry)| {
                (
                    key.clone(),
                    entry.accessed_at,
                    entry.created_at,
                    entry.size_bytes,
                )
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(_, accessed_at, created_at, _)| (*accessed_at, *created_at));

        let mut remaining = current_size_bytes;
        let mut victims = Vec::new();

        for (key, _, _, size_bytes) in candidates {
            if remaining <= max_size_bytes {
                break;
            }

            victims.push(key);
            remaining = remaining.saturating_sub(size_bytes);
        }

        victims
    }
}
