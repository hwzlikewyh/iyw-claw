use std::path::Path;

use serde_json::Value;

use super::super::{
    input::prepare_images, invalid, ImageRequest, ImageSource, IywGatewayService, PreparedImage,
};
use crate::acp::delegation::image_format::{detect_mime, normalize_mime};

pub(in crate::acp::builtin_mcp::iyw_image) async fn prepare_edit_images(
    context: (&IywGatewayService, &Path),
    (request, kind): (&ImageRequest, &str),
    images: &mut Vec<PreparedImage>,
) -> Result<(), rmcp::ErrorData> {
    if let Some(mask) = request
        .parameters
        .get("mask")
        .and_then(Value::as_str)
        .filter(|_| kind == "edit")
    {
        if images.iter().any(|image| image.role == "mask") {
            return Err(invalid(
                "Provide the edit mask in images with role=mask or parameters.mask, not both",
            ));
        }
        let mut mask = prepare_images(context.1, &[ImageSource::String(mask.to_string())]).await?;
        mask[0].role = "mask".into();
        images.extend(mask);
    }
    for image in images.iter_mut() {
        prepare_image(context.0, image).await?;
    }
    validate_mask_dimensions(images)?;
    Ok(())
}

fn validate_mask_dimensions(images: &[PreparedImage]) -> Result<(), rmcp::ErrorData> {
    let Some(mask) = images.iter().find(|image| image.role == "mask") else {
        return Ok(());
    };
    let source = images
        .iter()
        .find(|image| image.role != "mask")
        .ok_or_else(|| invalid("edit requires a source image in addition to its mask"))?;
    if dimensions(mask)? != dimensions(source)? {
        return Err(invalid(
            "edit mask dimensions must match the first source image",
        ));
    }
    Ok(())
}

fn dimensions(image: &PreparedImage) -> Result<(u32, u32), rmcp::ErrorData> {
    image::ImageReader::new(std::io::Cursor::new(
        image.bytes.as_deref().expect("prepared edit image"),
    ))
    .with_guessed_format()
    .map_err(|_| invalid("edit image format could not be read"))?
    .into_dimensions()
    .map_err(|_| invalid("edit image dimensions could not be read"))
}

async fn prepare_image(
    service: &IywGatewayService,
    image: &mut PreparedImage,
) -> Result<(), rmcp::ErrorData> {
    if image.bytes.is_none() {
        image.bytes = Some(service.download_image(&image.url).await?);
    }
    let bytes = image.bytes.as_deref().expect("edit input downloaded");
    let mime =
        detect_mime(bytes).ok_or_else(|| invalid("edit input image format is unsupported"))?;
    if image
        .mime_type
        .as_deref()
        .is_some_and(|declared| normalize_mime(declared) != Some(mime))
    {
        return Err(invalid(
            "edit input MIME type does not match its image data",
        ));
    }
    if !matches!(mime, "image/png" | "image/jpeg" | "image/webp")
        || (image.role == "mask" && mime != "image/png")
    {
        return Err(invalid(
            "Fusion edit inputs must be PNG/JPEG/WebP; masks must be PNG",
        ));
    }
    image.mime_type = Some(mime.to_string());
    Ok(())
}
