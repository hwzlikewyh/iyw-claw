use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

const MAX_SCANNED_FILES: usize = 128;
const SCAN_TTL: Duration = Duration::from_secs(300);

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Revision {
    length: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
}

struct Entry {
    revision: Revision,
    checked_at: Instant,
}

fn cache() -> &'static Mutex<HashMap<PathBuf, Entry>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Entry>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

pub(super) fn revision(path: &Path) -> Option<Revision> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(Revision {
        length: metadata.len(),
        modified: metadata.modified().ok()?,
        created: metadata.created().ok(),
    })
}

pub(super) fn already_scanned(path: &Path, revision: Option<&Revision>) -> bool {
    let Some(revision) = revision else {
        return false;
    };
    let entries = cache().lock().unwrap_or_else(|error| error.into_inner());
    entries
        .get(path)
        .is_some_and(|entry| entry.checked_at.elapsed() < SCAN_TTL && entry.revision == *revision)
}

pub(super) fn remember_unchanged(path: &Path, scanned: Option<Revision>) {
    let Some(scanned) = scanned else {
        return;
    };
    if revision(path).as_ref() != Some(&scanned) {
        return;
    }
    let mut entries = cache().lock().unwrap_or_else(|error| error.into_inner());
    entries.retain(|_, entry| entry.checked_at.elapsed() < SCAN_TTL);
    if entries.len() >= MAX_SCANNED_FILES {
        let oldest = entries
            .iter()
            .min_by_key(|(_, entry)| entry.checked_at)
            .map(|(path, _)| path.clone());
        if let Some(oldest) = oldest {
            entries.remove(&oldest);
        }
    }
    entries.insert(
        path.to_path_buf(),
        Entry {
            revision: scanned,
            checked_at: Instant::now(),
        },
    );
}
