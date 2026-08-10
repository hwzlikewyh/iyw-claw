use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;
use serde_json::{json, Value};

use super::image_loader::{self, ImageLoadRequest};

#[derive(Debug, Deserialize)]
struct ShowImageArguments {
    source: String,
    mime_type: Option<String>,
    caption: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnalyzeImageArguments {
    source: String,
    question: String,
    detail: Option<String>,
    mime_type: Option<String>,
}

pub struct PreparedImageAnalysis {
    pub data: String,
    pub mime_type: String,
    pub question: String,
    pub detail: String,
    pub image_bytes: usize,
}

pub async fn execute(arguments: Value, working_dir: PathBuf) -> Value {
    match show_image(arguments, &working_dir).await {
        Ok(result) => result,
        Err(error) => json!({
            "content": [{ "type": "text", "text": error }],
            "isError": true,
        }),
    }
}

async fn show_image(arguments: Value, working_dir: &Path) -> Result<Value, String> {
    let args: ShowImageArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("invalid show_image arguments: {error}"))?;
    validate_show_fields(&args)?;
    let image = image_loader::load(
        ImageLoadRequest {
            source: &args.source,
            mime_type: args.mime_type.as_deref(),
            allow_http: true,
        },
        working_dir,
    )
    .await
    .map_err(|error| error.safe_message().to_string())?;
    let extension = match image.mime_type {
        "image/jpeg" => "jpg",
        "image/svg+xml" => "svg",
        _ => image.mime_type.strip_prefix("image/").unwrap_or("img"),
    };
    let name = args
        .name
        .or(image.name)
        .unwrap_or_else(|| format!("image.{extension}"));
    let metadata = json!({
        "type": "iyw_claw_display_image", "caption": args.caption, "name": name,
        "source_kind": image.source_kind, "source": image.source,
    });
    Ok(json!({
        "content": [
            { "type": "text", "text": metadata.to_string() },
            { "type": "image", "data": STANDARD.encode(image.bytes), "mimeType": image.mime_type },
        ],
        "isError": false,
    }))
}

pub async fn prepare_analysis(
    arguments: Value,
    working_dir: &Path,
) -> Result<PreparedImageAnalysis, Value> {
    let args: AnalyzeImageArguments = serde_json::from_value(arguments).map_err(|_| {
        analysis_error(
            "image_analysis_invalid_arguments",
            "The image analysis arguments are invalid.",
        )
    })?;
    let (question, detail) = validate_analysis_fields(&args)?;
    let image = image_loader::load(
        ImageLoadRequest {
            source: &args.source,
            mime_type: args.mime_type.as_deref(),
            allow_http: false,
        },
        working_dir,
    )
    .await
    .map_err(|error| analysis_error(error.code, error.safe_message()))?;
    validate_analysis_format(image.mime_type)?;
    Ok(PreparedImageAnalysis {
        data: STANDARD.encode(&image.bytes),
        mime_type: image.mime_type.to_string(),
        question,
        detail,
        image_bytes: image.bytes.len(),
    })
}

fn validate_analysis_fields(args: &AnalyzeImageArguments) -> Result<(String, String), Value> {
    let question = args.question.trim().to_string();
    if question.is_empty() || question.chars().count() > 4096 {
        return Err(analysis_error(
            "image_analysis_invalid_arguments",
            "The analysis question must contain between 1 and 4096 characters.",
        ));
    }
    let detail = args.detail.unwrap_or_else(|| "auto".into());
    if !matches!(detail.as_str(), "auto" | "low" | "high") {
        return Err(analysis_error(
            "image_analysis_invalid_arguments",
            "Image detail must be auto, low, or high.",
        ));
    }
    Ok((question, detail))
}

fn validate_analysis_format(mime_type: &str) -> Result<(), Value> {
    if !matches!(
        mime_type,
        "image/jpeg" | "image/png" | "image/webp" | "image/gif"
    ) {
        return Err(analysis_error(
            "image_unsupported_format",
            "Image analysis supports JPEG, PNG, WebP, and GIF images.",
        ));
    }
    Ok(())
}

fn validate_show_fields(args: &ShowImageArguments) -> Result<(), String> {
    if args.source.trim().is_empty() {
        return Err("source must not be empty".into());
    }
    if args
        .name
        .as_ref()
        .is_some_and(|name| name.chars().count() > 255)
    {
        return Err("name must not exceed 255 characters".into());
    }
    if args
        .caption
        .as_ref()
        .is_some_and(|caption| caption.chars().count() > 2000)
    {
        return Err("caption must not exceed 2000 characters".into());
    }
    Ok(())
}

fn analysis_error(code: &str, message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
        "structuredContent": { "code": code, "error": message },
    })
}
