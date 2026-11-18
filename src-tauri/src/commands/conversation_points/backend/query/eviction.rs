use super::{Cache, CACHE_LIMIT};

pub(super) fn reserve(cached: &mut Cache, ids: &[String]) {
    let added = ids
        .iter()
        .filter(|id| !cached.records.contains_key(*id))
        .count();
    let excess = cached
        .records
        .len()
        .saturating_add(added)
        .saturating_sub(CACHE_LIMIT);
    if excess == 0 {
        return;
    }
    let mut candidates: Vec<_> = cached
        .records
        .keys()
        .filter(|id| !ids.contains(id))
        .map(|id| (cached.retry_after.get(id).copied(), id.clone()))
        .collect();
    candidates.sort_unstable();
    for (_, id) in candidates.into_iter().take(excess) {
        cached.records.remove(&id);
        cached.retry_after.remove(&id);
        cached.persisted.remove(&id);
    }
    tracing::debug!(
        evicted = excess,
        "[conversation-points] bounded cache eviction"
    );
}
