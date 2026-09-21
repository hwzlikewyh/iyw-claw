use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

pub(super) const MODEL_ID: &str = "bge-small-zh-v1.5-313386ad-cls-f32-v1";
pub(super) const MODEL_REVISION: &str = "313386ad7ffad360a4870fd1910f8ecaf1a05151";
pub(super) const MODEL_COMPONENT_ID: &str = "memory-embedding-bge-small-zh-v1.5";
pub(super) const LEGACY_DIRECTORY: &str = ".memory-models/bge-small-zh-v1.5";
pub(super) const LEGACY_VERSION: &str = "1.0.0";
pub(super) const DIMENSION: usize = 512;
pub(super) const POOLING: &str = "cls";
pub(super) const MANIFEST_FILE: &str = "manifest.json";

#[derive(Clone, Copy)]
pub(super) struct ExpectedModelFile {
    pub name: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

pub(super) const MODEL_FILES: [ExpectedModelFile; 5] = [
    ExpectedModelFile {
        name: "model.onnx",
        sha256: "69a0b846f4f116b5e6aabf9546ea6754d02264f3211a13a1bd69b31b8040749a",
        size: 94_851_877,
    },
    ExpectedModelFile {
        name: "config.json",
        sha256: "d4193ead3a810fd694fa8a31d7fc72fbaebc0668b603e398734bf2f6538ff42f",
        size: 716,
    },
    ExpectedModelFile {
        name: "tokenizer.json",
        sha256: "48cea5d44424912a6fd1ea647bf4fe50b55ab8b1e5879c3275f80e339e8fae26",
        size: 439_125,
    },
    ExpectedModelFile {
        name: "special_tokens_map.json",
        sha256: "b6d346be366a7d1d48332dbc9fdf3bf8960b5d879522b7799ddba59e76237ee3",
        size: 125,
    },
    ExpectedModelFile {
        name: "tokenizer_config.json",
        sha256: "e6f3b96db926a37d4039995fbf5ad17de158dfb8f6343d607e4dbaad18d75f5a",
        size: 367,
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ModelManifest {
    pub schema_version: u32,
    pub model_id: String,
    pub display_name: String,
    pub upstream_revision: String,
    pub dimension: usize,
    pub pooling: String,
    pub files: Vec<ModelManifestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModelManifestFile {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

impl ModelManifest {
    pub(super) fn legacy() -> Self {
        Self {
            schema_version: 1,
            model_id: MODEL_ID.to_string(),
            display_name: "BGE Small 中文语义模型".to_string(),
            upstream_revision: MODEL_REVISION.to_string(),
            dimension: DIMENSION,
            pooling: POOLING.to_string(),
            files: MODEL_FILES
                .iter()
                .map(|file| ModelManifestFile {
                    name: file.name.to_string(),
                    size: file.size,
                    sha256: file.sha256.to_string(),
                })
                .collect(),
        }
    }

    pub(super) fn validate(&self) -> Result<(), AppCommandError> {
        if self.schema_version != 1
            || self.model_id != MODEL_ID
            || self.upstream_revision != MODEL_REVISION
            || self.dimension != DIMENSION
            || self.pooling != POOLING
            || self.display_name.trim().is_empty()
        {
            return Err(model_error("Managed model manifest identity is invalid"));
        }
        self.validate_files()
    }

    fn validate_files(&self) -> Result<(), AppCommandError> {
        if self.files.len() != MODEL_FILES.len() {
            return Err(model_error(
                "Managed model manifest file list is incomplete",
            ));
        }
        let mut seen = HashSet::new();
        for file in &self.files {
            let expected = MODEL_FILES.iter().find(|item| item.name == file.name);
            let valid = expected.is_some_and(|item| {
                file.size == item.size && file.sha256.eq_ignore_ascii_case(item.sha256)
            });
            if !valid || !seen.insert(file.name.as_str()) {
                return Err(model_error("Managed model manifest file entry is invalid"));
            }
        }
        Ok(())
    }
}

pub(super) fn model_error(detail: impl std::fmt::Display) -> AppCommandError {
    AppCommandError::configuration_invalid("本地语义检索暂时不可用")
        .with_detail(detail.to_string())
}
