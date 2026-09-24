use serde_json::{json, Value};

use super::artifact_targets::ArtifactChannelTarget;
use super::types::SendItemInput;
use crate::chat_channel::attachments::MAX_MESSAGE_ATTACHMENTS;
use crate::db::entities::task_artifact;
use crate::db::AppDatabase;

pub(super) struct ArtifactContent {
    pub text: String,
    pub files: Vec<String>,
    pub errors: Vec<Value>,
    _archives: Vec<super::artifact_archive::ArtifactArchive>,
}

pub(super) async fn notification_message(
    db: &AppDatabase,
    message: Option<&str>,
) -> Result<String, String> {
    if let Some(message) = message {
        return Ok(message.to_owned());
    }
    let language = crate::commands::chat_channel::get_chat_message_language_core(db).await
        .map_err(|error| {
            tracing::error!(error = %error, "[artifact-notifications] message language read failed");
            "SETTINGS_QUERY_FAILED"
        })?;
    Ok(if language.starts_with("zh") {
        "成果已交付"
    } else {
        "Artifacts delivered"
    }
    .into())
}

pub(super) fn send_items(
    target: &ArtifactChannelTarget,
    content: &ArtifactContent,
) -> Vec<SendItemInput> {
    let chunks = content
        .files
        .chunks(MAX_MESSAGE_ATTACHMENTS)
        .collect::<Vec<_>>();
    let chunks = if chunks.is_empty() {
        vec![&[][..]]
    } else {
        chunks
    };
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, files)| SendItemInput {
            channel_id: target.channel_id,
            target_id: target.target_id.clone(),
            text: (index == 0).then(|| content.text.clone()),
            rich: None,
            files: files.to_vec(),
        })
        .collect()
}

pub(super) async fn prepare(artifacts: &[task_artifact::Model], message: &str) -> ArtifactContent {
    let mut content = ArtifactContent {
        text: message.to_owned(),
        files: Vec::new(),
        errors: Vec::new(),
        _archives: Vec::new(),
    };
    for artifact in artifacts {
        content
            .text
            .push_str(&format!("\n{}", artifact.display_name));
        match artifact.kind.as_str() {
            "url" => content.text.push_str(&format!("\n{}", artifact.path)),
            "file" => content.files.push(artifact.path.clone()),
            "directory" => match super::artifact_archive::prepare(artifact.path.clone()).await {
                Ok(archive) => {
                    content
                        .files
                        .push(archive.path.to_string_lossy().into_owned());
                    content._archives.push(archive);
                }
                Err(error) => {
                    tracing::warn!(
                        artifact_id = artifact.id,
                        error,
                        "[artifact-notifications] directory preparation failed"
                    );
                    content
                        .errors
                        .push(json!({"artifact_id": artifact.id, "error": error}));
                }
            },
            _ => content
                .errors
                .push(json!({"artifact_id": artifact.id, "error": "ARTIFACT_KIND_UNSUPPORTED"})),
        }
    }
    content
}
