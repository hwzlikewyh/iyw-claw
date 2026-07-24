pub const MAX_IMAGE_PIXELS: u64 = 16_000_000;

pub struct ImageInfo {
    pub mime_type: &'static str,
    pub width: u32,
    pub height: u32,
}

pub fn inspect(bytes: &[u8]) -> Result<ImageInfo, &'static str> {
    let info = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        inspect_png(bytes)?
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        inspect_jpeg(bytes)?
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        inspect_gif(bytes)?
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        inspect_webp(bytes)?
    } else if bytes.starts_with(b"BM") {
        inspect_bmp(bytes)?
    } else {
        return Err("unsupported remote image format");
    };
    validate_pixels(info)
}

fn validate_pixels(info: ImageInfo) -> Result<ImageInfo, &'static str> {
    let pixels = u64::from(info.width) * u64::from(info.height);
    if info.width == 0 || info.height == 0 {
        return Err("remote image has invalid dimensions");
    }
    if pixels > MAX_IMAGE_PIXELS {
        return Err("remote image exceeds the 16 megapixel limit");
    }
    Ok(info)
}

fn inspect_png(bytes: &[u8]) -> Result<ImageInfo, &'static str> {
    if bytes.len() < 24 || &bytes[12..16] != b"IHDR" {
        return Err("invalid PNG header");
    }
    Ok(ImageInfo {
        mime_type: "image/png",
        width: u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
        height: u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
    })
}

fn inspect_gif(bytes: &[u8]) -> Result<ImageInfo, &'static str> {
    if bytes.len() < 10 {
        return Err("invalid GIF header");
    }
    Ok(ImageInfo {
        mime_type: "image/gif",
        width: u16::from_le_bytes(bytes[6..8].try_into().unwrap()).into(),
        height: u16::from_le_bytes(bytes[8..10].try_into().unwrap()).into(),
    })
}

fn inspect_bmp(bytes: &[u8]) -> Result<ImageInfo, &'static str> {
    if bytes.len() < 22 {
        return Err("invalid BMP header");
    }
    let dib_size = u32::from_le_bytes(bytes[14..18].try_into().unwrap());
    let (width, height) = if dib_size == 12 {
        (
            u16::from_le_bytes(bytes[18..20].try_into().unwrap()).into(),
            u16::from_le_bytes(bytes[20..22].try_into().unwrap()).into(),
        )
    } else if dib_size >= 40 && bytes.len() >= 26 {
        let width = i32::from_le_bytes(bytes[18..22].try_into().unwrap());
        let height = i32::from_le_bytes(bytes[22..26].try_into().unwrap());
        if width <= 0 || height == i32::MIN {
            return Err("invalid BMP dimensions");
        }
        (width as u32, height.unsigned_abs())
    } else {
        return Err("unsupported BMP header");
    };
    Ok(ImageInfo {
        mime_type: "image/bmp",
        width,
        height,
    })
}

fn inspect_jpeg(bytes: &[u8]) -> Result<ImageInfo, &'static str> {
    let mut index = 2;
    while index + 1 < bytes.len() {
        while index < bytes.len() && bytes[index] == 0xff {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let marker = bytes[index];
        index += 1;
        if marker == 0xd8 || marker == 0xd9 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        if index + 2 > bytes.len() {
            break;
        }
        let length = usize::from(u16::from_be_bytes(
            bytes[index..index + 2].try_into().unwrap(),
        ));
        if length < 2 || index + length > bytes.len() {
            return Err("invalid JPEG segment");
        }
        if is_jpeg_start_of_frame(marker) && length >= 7 {
            return Ok(ImageInfo {
                mime_type: "image/jpeg",
                height: u16::from_be_bytes(bytes[index + 3..index + 5].try_into().unwrap()).into(),
                width: u16::from_be_bytes(bytes[index + 5..index + 7].try_into().unwrap()).into(),
            });
        }
        index += length;
    }
    Err("JPEG dimensions are missing")
}

fn is_jpeg_start_of_frame(marker: u8) -> bool {
    matches!(
        marker,
        0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
    )
}

fn inspect_webp(bytes: &[u8]) -> Result<ImageInfo, &'static str> {
    if bytes.len() < 30 {
        return Err("invalid WebP header");
    }
    let (width, height) = match &bytes[12..16] {
        b"VP8X" => (
            read_u24_le(&bytes[24..27]) + 1,
            read_u24_le(&bytes[27..30]) + 1,
        ),
        b"VP8L" if bytes[20] == 0x2f => {
            let bits = u32::from_le_bytes(bytes[21..25].try_into().unwrap());
            ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
        }
        b"VP8 " if &bytes[23..26] == b"\x9d\x01\x2a" => (
            u16::from_le_bytes(bytes[26..28].try_into().unwrap()) as u32 & 0x3fff,
            u16::from_le_bytes(bytes[28..30].try_into().unwrap()) as u32 & 0x3fff,
        ),
        _ => return Err("unsupported WebP header"),
    };
    Ok(ImageInfo {
        mime_type: "image/webp",
        width,
        height,
    })
}

fn read_u24_le(bytes: &[u8]) -> u32 {
    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16)
}
