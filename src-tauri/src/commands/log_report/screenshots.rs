use base64::Engine;
use image::ImageFormat;

use super::invalid;
use crate::app_error::AppCommandError;

const MAX_COUNT: usize = 5;
const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 20 * 1024 * 1024;
const MAX_ENCODED_BYTES: usize = MAX_TOTAL_BYTES.div_ceil(3) * 4 + (MAX_COUNT - 1) * 4;

pub(super) fn validate_encoded(images: &[String]) -> Result<(), AppCommandError> {
    if images.len() > MAX_COUNT || images.iter().map(String::len).sum::<usize>() > MAX_ENCODED_BYTES
    {
        return Err(invalid(
            "Screenshots exceed the count or total size limit",
            "screenshots",
        ));
    }
    Ok(())
}

pub(super) struct Screenshot {
    pub path: String,
    pub bytes: Vec<u8>,
}

pub(super) fn decode(images: Vec<String>) -> Result<Vec<Screenshot>, AppCommandError> {
    validate_encoded(&images)?;
    let mut decoded = Vec::new();
    let mut total = 0;
    for (index, encoded) in images.into_iter().enumerate() {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| invalid("Screenshot contains invalid Base64", "screenshots"))?;
        total += bytes.len();
        if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES || total > MAX_TOTAL_BYTES {
            return Err(invalid("Screenshot exceeds the size limit", "screenshots"));
        }
        let extension = match image::guess_format(&bytes).ok() {
            Some(ImageFormat::Png) => "png",
            Some(ImageFormat::Jpeg) => "jpg",
            Some(ImageFormat::WebP) => "webp",
            _ => {
                return Err(invalid(
                    "Screenshot must be PNG, JPEG or WebP",
                    "screenshots",
                ))
            }
        };
        let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|_| invalid("Screenshot format is invalid", "screenshots"))?;
        let (width, height) = reader
            .into_dimensions()
            .map_err(|_| invalid("Screenshot image is invalid", "screenshots"))?;
        if width == 0 || height == 0 {
            return Err(invalid("Screenshot is empty", "screenshots"));
        }
        decoded.push(Screenshot {
            path: format!("screenshots/{}.{}", index + 1, extension),
            bytes,
        });
    }
    Ok(decoded)
}
