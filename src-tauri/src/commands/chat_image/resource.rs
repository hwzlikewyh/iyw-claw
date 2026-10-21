//! 已开启引用的星河附件复用 Fusion 图床，避免再次上传到旧公开图床。
use std::sync::OnceLock;
use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{ChatImageResourceContext, EncodedChatImage};
use crate::app_error::AppCommandError;
use crate::commands::chat_image_upload::ImageUploadOutcome;

const PREPARE_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Deserialize)]
struct ResourceResponse {
    #[serde(default)]
    supported: bool,
    #[serde(default)]
    found: bool,
    image_url: Option<String>,
    sha256: Option<String>,
    size_bytes: Option<usize>,
}

pub(super) async fn prepare_managed(
    db: &DatabaseConnection,
    image: &EncodedChatImage,
    context: Option<&ChatImageResourceContext>,
) -> Result<Option<ImageUploadOutcome>, AppCommandError> {
    let Some(context) =
        context.filter(|context| context.agent_type == Some(crate::models::AgentType::Codex))
    else {
        return Ok(None);
    };
    let Some(token) = crate::commands::iyw_account::iyw_account_access_token_core(db).await? else {
        return Ok(None);
    };
    let default_model = crate::acp::provider_overlay::managed_default_model_for(
        crate::models::AgentType::Codex,
    );
    let model = context
        .model_id
        .as_deref()
        .filter(|model| !model.is_empty())
        .unwrap_or(default_model);
    let model = model.strip_prefix("iyw-claw/").unwrap_or(model);
    let Some(resource) = lookup_or_upload(token.expose(), image, model).await? else {
        return Ok(Some(ImageUploadOutcome::Unavailable(
            AppCommandError::network("图片资源服务暂时不可用"),
        )));
    };
    finish_resource(db, image, resource).await?;
    Ok(Some(ImageUploadOutcome::Managed))
}

async fn lookup_or_upload(
    token: &str,
    image: &EncodedChatImage,
    model: &str,
) -> Result<Option<ResourceResponse>, AppCommandError> {
    let digest = format!("{:x}", Sha256::digest(&image.bytes));
    let mut payload =
        json!({"operation":"lookup", "model":model, "sha256":digest, "mime_type":image.mime_type});
    let Some(resource) = request(token, &payload).await? else {
        return Ok(None);
    };
    if resource.found {
        return Ok(Some(resource));
    }
    payload["operation"] = "upload".into();
    payload["data"] = STANDARD.encode(&image.bytes).into();
    request(token, &payload).await
}

async fn finish_resource(
    db: &DatabaseConnection,
    image: &EncodedChatImage,
    resource: ResourceResponse,
) -> Result<(), AppCommandError> {
    let digest = format!("{:x}", Sha256::digest(&image.bytes));
    if !resource.found
        || resource.sha256.as_deref() != Some(&digest)
        || resource.size_bytes != Some(image.bytes.len())
    {
        return Err(AppCommandError::network(
            "Prepared image resource did not match the attachment",
        ));
    }
    let url = resource
        .image_url
        .ok_or_else(|| AppCommandError::network("Prepared image resource has no access URL"))?;
    super::super::chat_image_upload::check_managed_image(db, &url).await?;
    Ok(())
}

async fn request(
    token: &str,
    payload: &serde_json::Value,
) -> Result<Option<ResourceResponse>, AppCommandError> {
    static CLIENT: OnceLock<Result<reqwest::Client, reqwest::Error>> = OnceLock::new();
    let client = CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()
        })
        .as_ref()
        .map_err(|_| AppCommandError::network("本地图片请求客户端初始化失败"))?;
    let url = crate::acp::model_gateway::endpoint("images/input/prepare")
        .ok_or_else(|| AppCommandError::network("本地图片网关未就绪，请重新连接会话"))?;
    let response = client
        .post(url)
        .header("token", token)
        .timeout(PREPARE_TIMEOUT)
        .json(payload)
        .send()
        .await
        .map_err(|_| AppCommandError::network("本地图片资源准备请求失败"))?;
    read_response(response, payload["operation"].as_str().unwrap_or("unknown")).await
}

async fn read_response(
    response: reqwest::Response,
    operation: &str,
) -> Result<Option<ResourceResponse>, AppCommandError> {
    let status = response.status();
    if !status.is_success() {
        // 仅临时资源服务故障保留本地图片；鉴权、参数和内容校验错误仍向上传递。
        let temporary = matches!(
            status,
            reqwest::StatusCode::BAD_GATEWAY
                | reqwest::StatusCode::SERVICE_UNAVAILABLE
                | reqwest::StatusCode::GATEWAY_TIMEOUT
        );
        tracing::warn!(target: "chat.image", status = status.as_u16(), operation, temporary,
            "managed image preparation failed before model execution");
        if temporary {
            return Ok(None);
        }
        return Err(
            AppCommandError::network("图片资源准备失败").with_detail(format!("HTTP {status}"))
        );
    }
    let resource: ResourceResponse = response
        .json()
        .await
        .map_err(|_| AppCommandError::network("图片资源响应格式无效"))?;
    if !resource.supported {
        return Err(AppCommandError::configuration_invalid(
            "当前模型未启用图片资源支持",
        ));
    }
    Ok(Some(resource))
}
