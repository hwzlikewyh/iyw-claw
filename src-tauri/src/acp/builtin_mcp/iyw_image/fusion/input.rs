use std::path::Path;

use serde_json::Value;

use super::super::{
    input::{prepare_images, ImageValidation},
    invalid, ImageRequest, ImageSource, IywGatewayService, PreparedImage,
};

pub(in crate::acp::builtin_mcp::iyw_image) async fn prepare_edit_images(
    context: (&IywGatewayService, &Path),
    (request, kind): (&ImageRequest, &str),
) -> Result<Vec<PreparedImage>, rmcp::ErrorData> {
    let mask = request
        .parameters
        .get("mask")
        .and_then(Value::as_str)
        .filter(|_| kind == "edit");
    if mask.is_some() && request.images.iter().any(|image| image.role() == "mask") {
        return Err(invalid(
            "Provide the edit mask in images with role=mask or parameters.mask, not both",
        ));
    }
    let mask_sources = mask
        .map(|value| vec![ImageSource::String(value.to_string())])
        .unwrap_or_default();
    let validation = if mask.is_some() || request.images.iter().any(|image| image.role() == "mask")
    {
        ImageValidation::MaskedEdit
    } else {
        ImageValidation::Edit
    };
    let (images, masks) = tokio::join!(
        prepare_images(context, &request.images, validation),
        prepare_images(context, &mask_sources, ImageValidation::MaskedEdit)
    );
    let mut images = images?;
    let mut masks = masks?;
    if let Some(mask) = masks.first_mut() {
        if mask.mime_type.as_deref() != Some("image/png") {
            return Err(invalid("Fusion edit masks must be PNG"));
        }
        mask.role = "mask".into();
    }
    images.extend(masks);
    validate_mask_dimensions(&images)?;
    Ok(images)
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
    image
        .dimensions
        .ok_or_else(|| invalid("edit image dimensions could not be read"))
}
