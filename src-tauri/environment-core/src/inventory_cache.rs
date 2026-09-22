use std::collections::BTreeSet;
use std::path::Path;

use crate::model::FileRecord;

pub(super) fn immutable_records<'a>(
    component_id: &str,
    records: &'a [FileRecord],
) -> impl Iterator<Item = &'a FileRecord> {
    let python_runtime = component_id == "agent-reach";
    let sources: BTreeSet<_> = records
        .iter()
        .filter(|record| record.link_target.is_none() && !record.sha256.is_empty())
        .map(|record| record.path.as_str())
        .collect();
    records.iter().filter(move |record| {
        // 只忽略存在受摘要保护源码的 CPython 缓存，不放过无源码的字节码或其他组件文件。
        !python_runtime
            || cached_source(&record.path).is_none_or(|source| !sources.contains(source.as_str()))
    })
}

fn cached_source(relative: &str) -> Option<String> {
    let path = Path::new(relative);
    if !path.starts_with("python") || path.extension()? != "pyc" {
        return None;
    }
    let cache = path.parent()?;
    if cache.file_name()? != "__pycache__" {
        return None;
    }
    let (module, _) = path.file_stem()?.to_str()?.split_once(".cpython-")?;
    let source = cache.parent()?.join(format!("{module}.py"));
    Some(source.to_string_lossy().replace('\\', "/"))
}
