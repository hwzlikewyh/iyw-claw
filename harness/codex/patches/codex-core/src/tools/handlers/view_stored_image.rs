//! 复用受管原图资源，不依赖临时截图目录或过期访问链接。
use codex_protocol::models::{
    FunctionCallOutputBody, FunctionCallOutputContentItem, FunctionCallOutputPayload, ImageDetail,
    ImageReference, ResponseInputItem,
};

use crate::function_tool::FunctionCallError;
use crate::session::session::Session;
use crate::tools::context::{ToolOutput, ToolPayload};

pub(super) async fn load(
    session: &Session,
    request: StoredImageRequest,
) -> Result<StoredImageOutput, FunctionCallError> {
    let ImageReference::File { file_id } = &request.image else {
        return Err(FunctionCallError::RespondToModel(
            "invalid managed image reference".into(),
        ));
    };
    let image_url = session
        .services
        .model_client
        .read_managed_image(file_id)
        .await
        .map_err(|error| FunctionCallError::RespondToModel(error.to_string()))?;
    Ok(StoredImageOutput { request, image_url })
}

pub(super) struct StoredImageRequest {
    pub image: ImageReference,
    pub detail: ImageDetail,
    pub unified_image_budget: bool,
}

pub(super) struct StoredImageOutput {
    request: StoredImageRequest,
    image_url: String,
}

impl ToolOutput for StoredImageOutput {
    fn log_output(&self) -> String {
        "managed image resource loaded".into()
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, _payload: &ToolPayload) -> ResponseInputItem {
        ResponseInputItem::FunctionCallOutput {
            call_id: call_id.to_owned(),
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::ContentItems(vec![
                    FunctionCallOutputContentItem::InputImage {
                        image: self.request.image.clone(),
                        detail: Some(self.request.detail),
                    },
                ]),
                success: Some(true),
            },
        }
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> serde_json::Value {
        let mut value = serde_json::json!({"image_url":self.image_url});
        if !self.request.unified_image_budget {
            value["detail"] = serde_json::json!(self.request.detail);
        }
        value
    }
}
