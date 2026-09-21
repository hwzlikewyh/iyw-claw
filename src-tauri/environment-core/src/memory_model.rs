use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};

pub const COMPONENT: &str = "memory-embedding-bge-small-zh-v1.5";
pub const MODEL_ID: &str = "bge-small-zh-v1.5-313386ad-cls-f32-v1";
const REVISION: &str = "313386ad7ffad360a4870fd1910f8ecaf1a05151";
const FILES: &[(&str, u64, &str)] = &[
    (
        "model.onnx",
        94_851_877,
        "69a0b846f4f116b5e6aabf9546ea6754d02264f3211a13a1bd69b31b8040749a",
    ),
    (
        "config.json",
        716,
        "d4193ead3a810fd694fa8a31d7fc72fbaebc0668b603e398734bf2f6538ff42f",
    ),
    (
        "tokenizer.json",
        439_125,
        "48cea5d44424912a6fd1ea647bf4fe50b55ab8b1e5879c3275f80e339e8fae26",
    ),
    (
        "special_tokens_map.json",
        125,
        "b6d346be366a7d1d48332dbc9fdf3bf8960b5d879522b7799ddba59e76237ee3",
    ),
    (
        "tokenizer_config.json",
        367,
        "e6f3b96db926a37d4039995fbf5ad17de158dfb8f6343d607e4dbaad18d75f5a",
    ),
];

pub fn verify(root: &Path) -> Result<BTreeMap<String, String>> {
    let raw = std::fs::read(root.join("manifest.json")).context("BGE 模型清单缺失")?;
    let manifest: serde_json::Value = serde_json::from_slice(&raw)?;
    if manifest["schemaVersion"] != 1
        || manifest["modelId"] != MODEL_ID
        || manifest["upstreamRevision"] != REVISION
        || manifest["dimension"] != 512
        || manifest["pooling"] != "cls"
    {
        bail!("BGE 模型身份或维度不符")
    }
    for (name, size, hash) in FILES {
        if !crate::download::valid_file(&root.join(name), *size, hash)? {
            bail!("BGE 模型文件 {name} 缺失或校验失败")
        }
    }
    Ok(BTreeMap::from([("model".into(), "model.onnx".into())]))
}
