//! 将额外的源图片格式转换为模型可接收的 PNG，不改写源文件。
use std::io::{self, Cursor, Write};
use std::path::Path;

use image::{
    codecs::png::PngEncoder, DynamicImage, ImageDecoder, ImageEncoder, ImageFormat, ImageReader,
    Limits,
};

use super::formats::{decode_error, dimension_error, raster_error, unsupported_source};
use super::{
    EncodedChatImage, CHAT_IMAGE_DECODE_MAX_ALLOC, CHAT_IMAGE_DECODE_MAX_EDGE,
    CHAT_IMAGE_INPUT_MAX_BYTES,
};
use crate::app_error::AppCommandError;

const CONVERSION_BYTES_PER_PIXEL: u64 = 16;

pub(super) fn limited_reader(bytes: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>, AppCommandError> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(decode_error)?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(CHAT_IMAGE_DECODE_MAX_EDGE);
    limits.max_image_height = Some(CHAT_IMAGE_DECODE_MAX_EDGE);
    limits.max_alloc = Some(CHAT_IMAGE_DECODE_MAX_ALLOC);
    reader.limits(limits);
    Ok(reader)
}

pub(super) fn validate_dimensions(
    width: u32,
    height: u32,
    bytes_per_pixel: u64,
) -> Result<(), AppCommandError> {
    if width == 0
        || height == 0
        || width > CHAT_IMAGE_DECODE_MAX_EDGE
        || height > CHAT_IMAGE_DECODE_MAX_EDGE
    {
        return Err(dimension_error());
    }
    if u64::from(width) * u64::from(height) * bytes_per_pixel > CHAT_IMAGE_DECODE_MAX_ALLOC {
        return Err(dimension_error());
    }
    Ok(())
}

pub(super) fn prepare(
    bytes: Vec<u8>,
    source_bytes: u64,
    name: String,
) -> Result<EncodedChatImage, AppCommandError> {
    if bytes.is_empty() {
        return Err(decode_error("empty image"));
    }
    if let Some(format) = image::guess_format(&bytes)
        .ok()
        .or_else(|| supplemental_format(&bytes, &name))
    {
        if super::supported_mime(format).is_ok() {
            return original(bytes, source_bytes, name);
        }
        return raster(bytes, format, (source_bytes, name));
    }
    let image = if super::special::is_heif(&bytes) {
        super::special::heif(&bytes)?
    } else if super::special::is_svg(&bytes) {
        super::special::svg(&bytes)?
    } else {
        return Err(unsupported_source());
    };
    converted(image, source_bytes, name)
}

fn supplemental_format(bytes: &[u8], name: &str) -> Option<ImageFormat> {
    if bytes.starts_with(&[0, 0, 2, 0]) {
        return Some(ImageFormat::Ico); // CUR 与 ICO 共用解码器。
    }
    if bytes.starts_with(b"II+\0") || bytes.starts_with(b"MM\0+") {
        return Some(ImageFormat::Tiff); // BigTIFF 的字节标记不同。
    }
    let extension = Path::new(name).extension()?.to_str()?;
    extension
        .eq_ignore_ascii_case("tga")
        .then_some(ImageFormat::Tga)
}

fn original(
    bytes: Vec<u8>,
    source_bytes: u64,
    name: String,
) -> Result<EncodedChatImage, AppCommandError> {
    validate_input_size(bytes.len())?;
    let (format, width, height) = super::inspect_image(&bytes)?;
    Ok(EncodedChatImage {
        bytes,
        source_bytes,
        name,
        width,
        height,
        mime_type: super::supported_mime(format)?,
    })
}

fn raster(
    bytes: Vec<u8>,
    format: ImageFormat,
    (source_bytes, name): (u64, String),
) -> Result<EncodedChatImage, AppCommandError> {
    tracing::info!(target: "chat.image", source_format = ?format, source_bytes,
        "decoding source image for PNG conversion");
    if !matches!(
        format,
        ImageFormat::Bmp
            | ImageFormat::Tiff
            | ImageFormat::Ico
            | ImageFormat::Pnm
            | ImageFormat::Tga
            | ImageFormat::Qoi
            | ImageFormat::Dds
    ) {
        return Err(unsupported_source());
    }
    let mut reader = limited_reader(&bytes)?;
    reader.set_format(format);
    let mut decoder = reader.into_decoder().map_err(raster_error)?;
    let (width, height) = decoder.dimensions();
    validate_dimensions(width, height, CONVERSION_BYTES_PER_PIXEL)?;
    let profile = decoder.icc_profile().map_err(decode_error)?;
    let orientation = decoder.orientation().map_err(decode_error)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(raster_error)?;
    image.apply_orientation(orientation);
    converted((image, profile), source_bytes, name)
}

fn converted(
    (image, profile): (DynamicImage, Option<Vec<u8>>),
    source_bytes: u64,
    name: String,
) -> Result<EncodedChatImage, AppCommandError> {
    let (width, height) = (image.width(), image.height());
    let bytes = png_bytes(image, profile)?;
    let stem = Path::new(&name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("image");
    tracing::info!(target: "chat.image", source_bytes, derived_bytes = bytes.len(), width, height,
        "converted source image to PNG for model input");
    Ok(EncodedChatImage {
        bytes,
        source_bytes,
        name: format!("{stem}.png"),
        width,
        height,
        mime_type: "image/png",
    })
}

fn png_bytes(image: DynamicImage, profile: Option<Vec<u8>>) -> Result<Vec<u8>, AppCommandError> {
    let mut writer = LimitedPngWriter {
        bytes: Vec::new(),
        exceeded: false,
    };
    let mut encoder = PngEncoder::new(&mut writer);
    if let Some(profile) = profile {
        encoder.set_icc_profile(profile).map_err(decode_error)?;
    }
    image.write_with_encoder(encoder).map_err(|error| {
        if writer.exceeded {
            return input_size_error();
        }
        decode_error(error)
    })?;
    validate_input_size(writer.bytes.len())?;
    Ok(writer.bytes)
}

pub(super) fn validate_input_size(bytes: usize) -> Result<(), AppCommandError> {
    if bytes > CHAT_IMAGE_INPUT_MAX_BYTES {
        return Err(input_size_error());
    }
    Ok(())
}

fn input_size_error() -> AppCommandError {
    AppCommandError::invalid_input(
        "Image model input exceeds 32 MiB; the original file has not been modified",
    )
    .with_i18n("errors.chatImage.inputTooLarge", Default::default())
}

struct LimitedPngWriter {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl Write for LimitedPngWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > CHAT_IMAGE_INPUT_MAX_BYTES {
            self.exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "PNG exceeds 32 MiB",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
