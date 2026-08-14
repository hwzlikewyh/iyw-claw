use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::acp::error::AcpError;
use crate::acp::types::PromptInputBlock;

pub(crate) async fn normalize_prompt_images_for_agent(
    blocks: Vec<PromptInputBlock>,
) -> Result<Vec<PromptInputBlock>, AcpError> {
    let mut normalized = Vec::with_capacity(blocks.len());
    for block in blocks {
        normalized.push(normalize_prompt_block(block).await?);
    }
    Ok(normalized)
}

async fn normalize_prompt_block(block: PromptInputBlock) -> Result<PromptInputBlock, AcpError> {
    let PromptInputBlock::Image {
        data,
        mime_type,
        uri,
    } = block
    else {
        return Ok(block);
    };
    let Some(uri) = uri else {
        return Ok(PromptInputBlock::Image {
            data,
            mime_type,
            uri: None,
        });
    };
    if !data.is_empty() {
        return Ok(PromptInputBlock::Image {
            data,
            mime_type,
            uri: None,
        });
    }
    if !uri.starts_with("https://") {
        return Err(AcpError::protocol(
            "Agent image input requires an HTTPS image URL.",
        ));
    }
    let downloaded = crate::remote_image::network::download(
        &uri,
        crate::acp::delegation::image_loader::MAX_IMAGE_BYTES,
    )
    .await
    .map_err(|_| AcpError::protocol("The remote image could not be loaded."))?;
    let detected = crate::acp::delegation::image_format::detect_mime(&downloaded.bytes)
        .ok_or_else(|| AcpError::protocol("The remote image format is invalid."))?;
    Ok(PromptInputBlock::Image {
        data: STANDARD.encode(downloaded.bytes),
        mime_type: detected.to_string(),
        uri: None,
    })
}
