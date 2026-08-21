use std::sync::atomic::{AtomicI64, Ordering};

static MAX_CONCURRENT_THREADS: AtomicI64 = AtomicI64::new(40);
const MIN_WAIT_TIMEOUT_MS: i64 = 10_000;
const DEFAULT_WAIT_TIMEOUT_MS: i64 = 30_000;
const MAX_WAIT_TIMEOUT_MS: i64 = 120_000;

pub(super) fn patch_toml(root: &mut toml::map::Map<String, toml::Value>) -> Result<(), String> {
    root.remove("agents");
    let features = table_entry(root, "features")?;
    let v2 = table_entry(features, "multi_agent_v2")?;
    v2.insert("enabled".into(), toml::Value::Boolean(true));
    v2.insert(
        "hide_spawn_agent_metadata".into(),
        toml::Value::Boolean(true),
    );
    v2.insert(
        "tool_namespace".into(),
        toml::Value::String("agents".into()),
    );
    v2.insert(
        "max_concurrent_threads_per_session".into(),
        toml::Value::Integer(max_concurrent_threads()),
    );
    v2.insert(
        "min_wait_timeout_ms".into(),
        toml::Value::Integer(MIN_WAIT_TIMEOUT_MS),
    );
    v2.insert(
        "default_wait_timeout_ms".into(),
        toml::Value::Integer(DEFAULT_WAIT_TIMEOUT_MS),
    );
    v2.insert(
        "max_wait_timeout_ms".into(),
        toml::Value::Integer(MAX_WAIT_TIMEOUT_MS),
    );
    Ok(())
}

pub(super) fn is_current(value: &toml::Value) -> bool {
    let root = match value.as_table() {
        Some(root) if !root.contains_key("agents") => root,
        _ => return false,
    };
    root.get("features")
        .and_then(|value| value.get("multi_agent_v2"))
        .is_some_and(values_match)
}

fn values_match(value: &toml::Value) -> bool {
    value.get("enabled").and_then(toml::Value::as_bool) == Some(true)
        && value
            .get("hide_spawn_agent_metadata")
            .and_then(toml::Value::as_bool)
            == Some(true)
        && value.get("tool_namespace").and_then(toml::Value::as_str) == Some("agents")
        && integer(value, "max_concurrent_threads_per_session") == Some(max_concurrent_threads())
        && integer(value, "min_wait_timeout_ms") == Some(MIN_WAIT_TIMEOUT_MS)
        && integer(value, "default_wait_timeout_ms") == Some(DEFAULT_WAIT_TIMEOUT_MS)
        && integer(value, "max_wait_timeout_ms") == Some(MAX_WAIT_TIMEOUT_MS)
}

fn max_concurrent_threads() -> i64 {
    MAX_CONCURRENT_THREADS.load(Ordering::Relaxed)
}

pub(crate) fn set_max_concurrent_threads(limit: u32) {
    MAX_CONCURRENT_THREADS.store(i64::from(limit), Ordering::Relaxed);
}

fn integer(value: &toml::Value, key: &str) -> Option<i64> {
    value.get(key).and_then(toml::Value::as_integer)
}

fn table_entry<'a>(
    table: &'a mut toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<&'a mut toml::map::Map<String, toml::Value>, String> {
    let value = table
        .entry(key)
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    if !value.is_table() {
        *value = toml::Value::Table(toml::map::Map::new());
    }
    value
        .as_table_mut()
        .ok_or_else(|| format!("{key} must be a TOML table"))
}
