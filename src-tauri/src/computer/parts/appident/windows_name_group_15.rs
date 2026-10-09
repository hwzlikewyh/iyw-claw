// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Windows: what to call the application whose executable is at `path` —
/// `app_user_model_id` names it when it is a packaged one. Read once and
/// kept (see the module note), on a thread of its own: a listing waits for it
/// [`NAME_WAIT`] at most — no longer at all for a while after one has not
/// come in time ([`NAME_SLOW`]) — and meanwhile goes by the file name, so a
/// shell that does not answer cannot hold a listing up. A name still being
/// read is not asked for again, no more than [`MAX_NAME_READERS`] are read at
/// once, and one is kept when it comes, for the listings after.
#[cfg(windows)]
pub(super) fn windows_name(path: &str, app_user_model_id: Option<&str>) -> String {
    use std::collections::{HashMap, HashSet};
    use std::sync::{mpsc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    type Key = (String, Option<String>);
    #[derive(Default)]
    struct Names {
        pub(in crate::computer::appident) known: HashMap<Key, String>,
        /// Being read by a thread that has not answered yet.
        pub(in crate::computer::appident) asking: HashSet<Key>,
        /// Until when no listing waits for a name.
        pub(in crate::computer::appident) slow_until: Option<Instant>,
    }
    static NAMES: OnceLock<Mutex<Names>> = OnceLock::new();
    let names: &'static Mutex<Names> = NAMES.get_or_init(Mutex::default);
    let key: Key = (path.to_string(), app_user_model_id.map(str::to_string));
    let wait = {
        let Ok(mut held) = names.lock() else {
            return file_name_of(path);
        };
        if let Some(name) = held.known.get(&key) {
            return name.clone();
        }
        if held.asking.len() >= MAX_NAME_READERS || !held.asking.insert(key.clone()) {
            return file_name_of(path);
        }
        if held.slow_until.is_some_and(|until| Instant::now() < until) {
            Duration::ZERO
        } else {
            NAME_WAIT
        }
    };
    let (told, answer) = mpsc::channel();
    let asked = key.clone();
    let reader = std::thread::Builder::new()
        .name("iyw-claw-app-name".into())
        .spawn(move || {
            let (path, id) = &asked;
            let name = id
                .as_deref()
                .and_then(windows_names::start_menu_name)
                .or_else(|| windows_names::file_description(path))
                .unwrap_or_else(|| file_name_of(path));
            if let Ok(mut held) = names.lock() {
                held.asking.remove(&asked);
                if held.known.len() >= MAX_WINDOWS_NAMES {
                    held.known.clear();
                }
                held.known.insert(asked.clone(), name.clone());
            }
            let _ = told.send(name);
        });
    if reader.is_err() {
        if let Ok(mut held) = names.lock() {
            held.asking.remove(&key);
        }
        return file_name_of(path);
    }
    match answer.recv_timeout(wait) {
        Ok(name) => name,
        Err(_) => {
            if !wait.is_zero() {
                if let Ok(mut held) = names.lock() {
                    held.slow_until = Some(Instant::now() + NAME_SLOW);
                }
            }
            file_name_of(path)
        }
    }
}

/// Windows: the file name of the executable at `path` — what an application
/// is called when nothing better says.
#[cfg(windows)]
pub(super) fn file_name_of(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .filter(|file| !file.is_empty())
        .unwrap_or(path)
        .to_string()
}
