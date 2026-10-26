//! HEIF 与 SVG 使用可随应用构建的解码器，不依赖宿主系统编解码组件。
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};

use image::{DynamicImage, ImageBuffer, Rgba};
use resvg::{tiny_skia, usvg};

use super::formats::decode_error;
use super::source::validate_dimensions;
use crate::app_error::AppCommandError;

const HEIF_BYTES_PER_PIXEL: u64 = 32;
const SVG_BYTES_PER_PIXEL: u64 = 16;

pub(super) fn is_heif(bytes: &[u8]) -> bool {
    bytes.get(4..8) == Some(b"ftyp")
        && bytes.get(8..12).is_some_and(|brand| {
            matches!(
                brand,
                b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1"
            )
        })
}

pub(super) fn is_svg(bytes: &[u8]) -> bool {
    let mut reader = quick_xml::Reader::from_reader(bytes);
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(tag) | quick_xml::events::Event::Empty(tag)) => {
                return tag.local_name().as_ref() == b"svg"
            }
            Ok(quick_xml::events::Event::Eof) | Err(_) => return false,
            _ => {}
        }
    }
}

pub(super) fn heif(bytes: &[u8]) -> Result<(DynamicImage, Option<Vec<u8>>), AppCommandError> {
    tracing::info!(target: "chat.image", source_format = "HEIF", "decoding source image for PNG conversion");
    let info = heic_decoder::probe(bytes).map_err(decode_error)?;
    validate_dimensions(info.width as u32, info.height as u32, HEIF_BYTES_PER_PIXEL)?;
    validate_dimensions(
        info.coded_width as u32,
        info.coded_height as u32,
        HEIF_BYTES_PER_PIXEL,
    )?;
    let decoded = heic_decoder::decode(bytes).map_err(decode_error)?;
    let profile = decoded.metadata.icc_profile.clone();
    let rgba = decoded.to_rgba16().map_err(decode_error)?;
    let mut data = rgba.data;
    if rgba.premultiplied {
        unpremultiply(&mut data);
    }
    let image = ImageBuffer::<Rgba<u16>, _>::from_raw(rgba.width as u32, rgba.height as u32, data)
        .ok_or_else(|| decode_error("HEIF pixel buffer dimensions do not match"))?;
    Ok((DynamicImage::ImageRgba16(image), profile))
}

fn unpremultiply(data: &mut [u16]) {
    const CHANNEL_MAX: u64 = u16::MAX as u64;
    for pixel in data.chunks_exact_mut(4) {
        let alpha = u64::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = if alpha == 0 {
                0
            } else {
                ((u64::from(*channel) * CHANNEL_MAX + alpha / 2) / alpha).min(CHANNEL_MAX) as u16
            };
        }
    }
}

pub(super) fn svg(bytes: &[u8]) -> Result<(DynamicImage, Option<Vec<u8>>), AppCommandError> {
    tracing::info!(target: "chat.image", source_format = "SVG", "rendering source image for PNG conversion");
    let blocked = Arc::new(AtomicBool::new(false));
    let options = svg_options(blocked.clone());
    let tree = usvg::Tree::from_data(bytes, &options).map_err(decode_error)?;
    if blocked.load(Ordering::Relaxed) {
        return Err(AppCommandError::invalid_input(
            "SVG references an external or invalid image; embed valid images as data URLs first",
        ));
    }
    let size = tree.size().to_int_size();
    validate_dimensions(size.width(), size.height(), SVG_BYTES_PER_PIXEL)?;
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| decode_error("SVG image buffer could not be allocated"))?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    let data = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let pixel = pixel.demultiply();
            [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
        })
        .collect();
    let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(size.width(), size.height(), data)
        .ok_or_else(|| decode_error("SVG pixel buffer dimensions do not match"))?;
    Ok((DynamicImage::ImageRgba8(image), None))
}

fn svg_options(blocked: Arc<AtomicBool>) -> usvg::Options<'static> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let fontdb = FONTS
        .get_or_init(|| {
            let mut fonts = usvg::fontdb::Database::new();
            fonts.load_system_fonts();
            Arc::new(fonts)
        })
        .clone();
    let blocked_data = blocked.clone();
    let resolver = usvg::ImageHrefResolver {
        resolve_string: Box::new(move |_, _| {
            blocked.store(true, Ordering::Relaxed);
            None
        }),
        resolve_data: Box::new(move |_, data, options| {
            let image = embedded_svg_image(data, options);
            if image.is_none() {
                blocked_data.store(true, Ordering::Relaxed);
            }
            image
        }),
    };
    usvg::Options {
        fontdb,
        image_href_resolver: resolver,
        ..Default::default()
    }
}

fn embedded_svg_image(data: Arc<Vec<u8>>, options: &usvg::Options) -> Option<usvg::ImageKind> {
    if is_svg(&data) {
        let tree = usvg::Tree::from_data(&data, options).ok()?;
        let size = tree.size().to_int_size();
        validate_dimensions(size.width(), size.height(), SVG_BYTES_PER_PIXEL).ok()?;
        return Some(usvg::ImageKind::SVG(tree));
    }
    let (format, width, height) = super::inspect_image(&data).ok()?;
    validate_dimensions(width, height, SVG_BYTES_PER_PIXEL).ok()?;
    super::source::limited_reader(&data).ok()?.decode().ok()?;
    let mime = super::supported_mime(format).ok()?;
    (usvg::ImageHrefResolver::default_data_resolver())(mime, data, options)
}
