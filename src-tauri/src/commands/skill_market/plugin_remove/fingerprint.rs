use crate::app_error::AppCommandError;
use sha2::{Digest, Sha256};

pub(super) fn token(prepared: &super::types::Prepared) -> Result<String, AppCommandError> {
    let mut records = prepared
        .records
        .iter()
        .map(|record| format!("{record:?}"))
        .collect::<Vec<_>>();
    records.sort();
    let pointers = prepared
        .pointers
        .iter()
        .map(|value| (&value.path, &value.previous, &value.next))
        .collect::<Vec<_>>();
    let markers = shared_markers()?;
    let root = super::super::plugin_history::local::root(&prepared.root.installation.slug)?;
    let pointer = super::super::plugin_storage_io::read_pointer_bytes(&root.join("current.json"))
        .ok()
        .flatten();
    let bytes = serde_json::to_vec(&serde_json::json!([
        prepared.scope,
        records,
        pointers,
        markers,
        pointer,
        prepared.cleanup
    ]))
    .map_err(|_| super::invalid("插件移除检查无法编码"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn shared_markers() -> Result<Vec<String>, AppCommandError> {
    let root = crate::commands::acp::shared_skills_dir();
    let entries = match std::fs::read_dir(root) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(super::invalid("无法检查共享技能引用")),
    };
    let mut markers = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= MAX_SHARED {
            return Err(super::invalid("共享技能过多，无法完整检查移除"));
        }
        let path = entry
            .map_err(|_| super::invalid("共享技能引用读取失败"))?
            .path();
        if let Some(marker) = super::super::plugin_history::local::read_marker(&path)? {
            markers.push(
                serde_json::to_string(&marker)
                    .map_err(|_| super::invalid("共享技能引用无法编码"))?,
            );
        }
    }
    markers.sort();
    Ok(markers)
}
const MAX_SHARED: usize = 4096;
