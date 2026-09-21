use std::path::Path;

use fastembed::{
    InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};

use crate::app_error::AppCommandError;

use super::managed_model_types::MODEL_FILES;
pub(super) use super::managed_model_types::{model_error, DIMENSION, MODEL_ID};
const MODEL_THREADS: usize = 2;

pub(super) fn installed(_root: &Path) -> bool {
    super::managed_model::installed(&crate::system_skills::data_dir_from_env())
}

pub(super) fn load(_root: &Path) -> Result<TextEmbedding, AppCommandError> {
    let directory =
        super::managed_model::model_directory(&crate::system_skills::data_dir_from_env())?;
    let mut files = Vec::new();
    for file in MODEL_FILES {
        files.push(std::fs::read(directory.join(file.name)).map_err(AppCommandError::io)?);
    }
    let tokenizer = TokenizerFiles {
        config_file: files[1].clone(),
        tokenizer_file: files[2].clone(),
        special_tokens_map_file: files[3].clone(),
        tokenizer_config_file: files[4].clone(),
    };
    let model = UserDefinedEmbeddingModel::new(std::mem::take(&mut files[0]), tokenizer)
        .with_pooling(Pooling::Cls);
    TextEmbedding::try_new_from_user_defined(
        model,
        InitOptionsUserDefined::new().with_intra_threads(MODEL_THREADS),
    )
    .map_err(super::managed_model_types::model_error)
}
