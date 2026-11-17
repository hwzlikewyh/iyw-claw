use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;

use super::SkillMarketUploadFile;
use crate::app_error::AppCommandError;

const PORTABLE: &str = ".iyw-plugin.json";
const NATIVE: [&str; 2] = [".codex-plugin/plugin.json", ".claude-plugin/plugin.json"];

// 只调整本次上传副本；保留目录中的原始文件和已经发布的制品。
pub(super) fn prepare_plugin_namespace(
    files: &mut [SkillMarketUploadFile],
    slug: &str,
    version: &str,
) -> Result<(), AppCommandError> {
    let index = files
        .iter()
        .position(|file| file.path == PORTABLE)
        .ok_or_else(|| invalid("缺少插件清单"))?;
    let mut manifest = document(&files[index])?;
    let name = manifest["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid("插件名称无效"))?
        .to_owned();
    if manifest["version"].as_str() != Some(version) {
        return Err(invalid("发布版本号必须与插件清单一致"));
    }
    if name == slug {
        return Ok(());
    }
    let mappings = namespace_capabilities(&mut manifest, &name, slug)?;
    let schema = manifest["schemaVersion"].as_u64();
    if schema == Some(1) {
        namespace_native(files, (&name, version), slug)?;
    }
    manifest["name"] = Value::String(slug.to_owned());
    write_document(&mut files[index], &manifest)?;
    rewrite_capability_references(files, &mappings)?;
    tracing::info!(
        files = files.len(),
        "[skill-market] upload plugin namespace prepared"
    );
    Ok(())
}

fn namespace_native(
    files: &mut [SkillMarketUploadFile],
    identity: (&str, &str),
    slug: &str,
) -> Result<(), AppCommandError> {
    let (name, version) = identity;
    for path in NATIVE {
        let file = files
            .iter_mut()
            .find(|file| file.path == path)
            .ok_or_else(|| invalid("缺少原生插件清单"))?;
        let mut native = document(file)?;
        if native["name"].as_str() != Some(name) || native["version"].as_str() != Some(version) {
            return Err(invalid("插件清单身份不一致"));
        }
        native["name"] = Value::String(slug.to_owned());
        write_document(file, &native)?;
    }
    Ok(())
}

fn namespace_capabilities(
    manifest: &mut Value,
    name: &str,
    slug: &str,
) -> Result<Vec<(String, String)>, AppCommandError> {
    if !matches!(manifest["schemaVersion"].as_u64(), Some(1 | 2)) {
        return Err(invalid("不支持的插件清单版本"));
    }
    let mut mappings = Vec::new();
    let Some(capabilities) = manifest
        .get_mut("components")
        .and_then(|value| value.get_mut("capabilities"))
        .and_then(Value::as_array_mut)
    else {
        return Ok(mappings);
    };
    let prefix = format!("plugin.{name}.");
    for capability in capabilities {
        let id = capability["id"]
            .as_str()
            .ok_or_else(|| invalid("插件能力标识无效"))?;
        let suffix = id
            .strip_prefix(&prefix)
            .ok_or_else(|| invalid("插件能力标识与插件名称不一致"))?;
        let replacement = format!("plugin.{slug}.{suffix}");
        mappings.push((id.to_owned(), replacement.clone()));
        capability["id"] = Value::String(replacement);
    }
    mappings.sort_by_key(|(old, _)| std::cmp::Reverse(old.len()));
    Ok(mappings)
}

fn document(file: &SkillMarketUploadFile) -> Result<Value, AppCommandError> {
    let bytes = decode(file)?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| invalid("插件清单必须是有效 JSON"))?;
    if !value.is_object() {
        return Err(invalid("插件清单必须是 JSON 对象"));
    }
    Ok(value)
}

fn decode(file: &SkillMarketUploadFile) -> Result<Vec<u8>, AppCommandError> {
    let bytes = STANDARD
        .decode(&file.content_base64)
        .map_err(|_| invalid("插件文件 Base64 无效"))?;
    if bytes.len() as u64 != file.size {
        return Err(invalid("插件文件大小与内容不一致"));
    }
    Ok(bytes)
}

fn write_document(file: &mut SkillMarketUploadFile, value: &Value) -> Result<(), AppCommandError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| invalid("无法生成插件上传清单"))?;
    write_bytes(file, &bytes);
    Ok(())
}

fn write_bytes(file: &mut SkillMarketUploadFile, bytes: &[u8]) {
    file.size = bytes.len() as u64;
    file.content_base64 = STANDARD.encode(bytes);
}

fn rewrite_capability_references(
    files: &mut [SkillMarketUploadFile],
    mappings: &[(String, String)],
) -> Result<(), AppCommandError> {
    if mappings.is_empty() {
        return Ok(());
    }
    for file in files.iter_mut().filter(|file| file.path != PORTABLE) {
        let bytes = decode(file)?;
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let mut updated = text.to_owned();
        for (old, new) in mappings {
            updated = updated.replace(old, new);
        }
        if updated != text {
            write_bytes(file, updated.as_bytes());
        }
    }
    Ok(())
}

fn invalid(message: &str) -> AppCommandError {
    AppCommandError::invalid_input(message)
}
