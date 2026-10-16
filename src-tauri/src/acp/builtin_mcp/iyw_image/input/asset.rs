use std::path::Path;

use sha2::{Digest, Sha256};

use crate::acp::delegation::image_loader::{self, ImageLoadRequest};

use super::{credential_free_https, extension_for_mime, invalid, IywGatewayService, PreparedImage};

pub(super) async fn prepare(
    (service, cwd): (&IywGatewayService, &Path),
    (source, mime): (&str, Option<&str>),
    (editing, inspect): (bool, bool),
) -> Result<PreparedImage, rmcp::ErrorData> {
    let inspect = inspect && source.starts_with("https://");
    if source.starts_with("https://") && !inspect {
        return load((service, cwd), (source, mime), (editing, inspect)).await;
    }
    let key = format!("{editing}:{inspect}:{}:{mime:?}", hex_digest(source));
    let operation = load((service, cwd), (source, mime), (editing, inspect));
    match &service.image_context {
        Some(context) => context.prepare(key, operation).await,
        None => operation.await,
    }
}

async fn load(
    (service, cwd): (&IywGatewayService, &Path),
    (source, mime): (&str, Option<&str>),
    (editing, inspect): (bool, bool),
) -> Result<PreparedImage, rmcp::ErrorData> {
    let remote = source.starts_with("https://");
    if remote && !inspect {
        return Ok(PreparedImage {
            url: credential_free_https(source)?,
            role: String::new(),
            mime_type: mime.map(str::to_owned),
            dimensions: None,
        });
    }
    let loaded = image_loader::load(
        ImageLoadRequest {
            source,
            mime_type: mime,
            allow_http: false,
        },
        cwd,
    )
    .await
    .map_err(|error| invalid(error.safe_message()))?;
    if editing && !matches!(loaded.mime_type, "image/png" | "image/jpeg" | "image/webp") {
        return Err(invalid(
            "Fusion edit inputs must be PNG/JPEG/WebP; masks must be PNG",
        ));
    }
    let dimensions = dimensions(&loaded.bytes);
    let url = if remote {
        credential_free_https(source)?
    } else {
        service
            .upload_bytes(
                loaded.bytes,
                loaded.mime_type,
                extension_for_mime(loaded.mime_type)?,
            )
            .await?
    };
    Ok(PreparedImage {
        url,
        role: String::new(),
        mime_type: Some(loaded.mime_type.to_string()),
        dimensions,
    })
}

fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

fn hex_digest(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}
