use std::time::Duration;

use chrono::Local;
use reqwest::{Client, Url};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::acp::capability_policy::monitor_file_upload;
use crate::app_error::AppCommandError;

use super::chat_image::{EncodedChatImage, PreparedChatImage};

const GATEWAY_ORIGIN: &str = "https://gateway.iyw.cn";
const IMAGE_API_PREFIX: &str = "/ai-application/api/microModel";
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Deserialize)]
struct GatewayEnvelope {
    code: i32,
    #[serde(default)]
    data: Value,
    message: Option<String>,
}

fn image_extension(mime_type: &str) -> Result<&'static str, AppCommandError> {
    match mime_type {
        "image/png" => Ok("png"),
        "image/jpeg" => Ok("jpg"),
        "image/webp" => Ok("webp"),
        _ => Err(AppCommandError::invalid_input(
            "Prepared image MIME type is not supported for upload",
        )),
    }
}

fn object_key(image: &EncodedChatImage) -> Result<String, AppCommandError> {
    let extension = image_extension(image.mime_type)?;
    Ok(format!(
        "AI/img/{}/{}.{}",
        Local::now().format("%y%m%d"),
        uuid::Uuid::new_v4().simple(),
        extension
    ))
}

fn endpoint(path: &str) -> String {
    format!("{GATEWAY_ORIGIN}{IMAGE_API_PREFIX}/{path}")
}

fn gateway_error(message: &str, payload: &GatewayEnvelope) -> AppCommandError {
    AppCommandError::network(message).with_detail(
        payload
            .message
            .as_deref()
            .unwrap_or("IYW image service rejected the request"),
    )
}

async fn post_gateway(
    client: &Client,
    token: &str,
    path: &str,
    body: Value,
) -> Result<GatewayEnvelope, AppCommandError> {
    let response = client
        .post(endpoint(path))
        .header("token", token)
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            AppCommandError::network("IYW image service request failed")
                .with_detail(error.to_string())
        })?;
    let status = response.status();
    let payload = response.json::<GatewayEnvelope>().await.map_err(|error| {
        AppCommandError::network("IYW image service returned invalid JSON")
            .with_detail(error.to_string())
    })?;
    if !status.is_success() || payload.code != 1 {
        return Err(gateway_error(
            "IYW image service rejected the request",
            &payload,
        ));
    }
    Ok(payload)
}

fn signed_url(payload: &GatewayEnvelope) -> Result<Url, AppCommandError> {
    let raw = payload
        .data
        .as_str()
        .or_else(|| payload.data.get("value").and_then(Value::as_str))
        .or_else(|| payload.data.get("url").and_then(Value::as_str))
        .ok_or_else(|| AppCommandError::network("Image upload URL is missing"))?;
    let url = Url::parse(raw).map_err(|error| {
        AppCommandError::network("Image upload URL is invalid").with_detail(error.to_string())
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(AppCommandError::network(
            "Image upload URL must be a credential-free HTTPS URL",
        ));
    }
    Ok(url)
}

fn public_url(mut signed: Url) -> Result<String, AppCommandError> {
    signed.set_query(None);
    signed.set_fragment(None);
    if signed.query().is_some() || signed.fragment().is_some() {
        return Err(AppCommandError::network(
            "Unable to derive a public image URL",
        ));
    }
    Ok(signed.to_string())
}

async fn put_image(
    client: &Client,
    signed: Url,
    image: &EncodedChatImage,
) -> Result<(), AppCommandError> {
    let response = client
        .put(signed)
        .header(reqwest::header::CONTENT_TYPE, image.mime_type)
        .body(image.bytes.clone())
        .send()
        .await
        .map_err(|error| {
            AppCommandError::network("Image upload failed").with_detail(error.to_string())
        })?;
    if !response.status().is_success() {
        return Err(AppCommandError::network("Image upload was rejected")
            .with_detail(response.status().to_string()));
    }
    Ok(())
}

pub(super) async fn upload_prepared(
    conn: &DatabaseConnection,
    image: &EncodedChatImage,
) -> Result<PreparedChatImage, AppCommandError> {
    let monitor = monitor_file_upload(None).await?;
    monitor
        .run_until_revoked(async {
            let token = crate::commands::iyw_account::iyw_account_access_token_core(conn)
                .await?
                .ok_or_else(|| {
                    AppCommandError::authentication_failed("Sign in to iyw-claw first")
                })?;
            let client = Client::builder()
                .timeout(UPLOAD_TIMEOUT)
                .user_agent("iyw-claw")
                .build()
                .map_err(|error| {
                    AppCommandError::network("Failed to initialize image upload")
                        .with_detail(error.to_string())
                })?;
            let key = object_key(image)?;
            let presigned = post_gateway(
                &client,
                token.expose(),
                "PreSignedUrl",
                json!({ "objectKey": key }),
            )
            .await?;
            let signed = signed_url(&presigned)?;
            let url = public_url(signed.clone())?;
            put_image(&client, signed, image).await?;
            post_gateway(
                &client,
                token.expose(),
                "checkImage",
                json!({ "image": url }),
            )
            .await?;
            Ok(PreparedChatImage {
                url,
                local_path: None,
                mime_type: image.mime_type.to_string(),
                name: image.name.clone(),
                source_bytes: image.source_bytes,
                derived_bytes: image.bytes.len(),
                width: image.width,
                height: image.height,
            })
        })
        .await?
}
