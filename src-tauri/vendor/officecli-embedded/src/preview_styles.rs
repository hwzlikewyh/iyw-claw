//! 从原文件补充只影响预览的样式，不改动 HCD 正文、锚点或哈希。
use std::io::Read;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

pub const FILE: &str = "office-preview.css";
const MAX_XML_BYTES: u64 = 8 * 1024 * 1024;
const PERCENT_UNIT: f64 = 100000.0;

pub fn write(source: &Path, output: &Path) -> anyhow::Result<()> {
    let extension = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    let css = if extension.eq_ignore_ascii_case("xlsx") {
        let mut archive = zip::ZipArchive::new(std::fs::File::open(source)?)?;
        if archive.file_names().any(|name| name == "xl/styles.xml") {
            spreadsheet(&read_part(&mut archive, "xl/styles.xml")?)?
        } else {
            String::new()
        }
    } else if extension.eq_ignore_ascii_case("pptx") {
        presentation(source)?
    } else {
        String::new()
    };
    std::fs::write(output.join(FILE), css)?;
    Ok(())
}

pub fn read(root: &Path) -> anyhow::Result<String> {
    match std::fs::read_to_string(root.join(FILE)) {
        Ok(css) => Ok(css),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

fn read_part(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> anyhow::Result<String> {
    let part = archive.by_name(name)?;
    anyhow::ensure!(
        part.size() <= MAX_XML_BYTES,
        "Office layout XML exceeds 8 MiB"
    );
    let mut xml = String::new();
    part.take(MAX_XML_BYTES + 1).read_to_string(&mut xml)?;
    anyhow::ensure!(
        xml.len() as u64 <= MAX_XML_BYTES,
        "Office layout XML exceeds 8 MiB"
    );
    Ok(xml)
}

fn number(element: &BytesStart<'_>, key: &[u8]) -> Option<f64> {
    element
        .attributes()
        .flatten()
        .find(|attr| attr.key.local_name().as_ref() == key)
        .and_then(|attr| std::str::from_utf8(&attr.value).ok()?.parse::<f64>().ok())
        .filter(|value| value.is_finite())
}

fn spreadsheet(xml: &str) -> anyhow::Result<String> {
    let mut reader = Reader::from_str(xml);
    let (mut inside, mut index, mut css) = (false, 0usize, String::new());
    loop {
        match reader.read_event()? {
            Event::Start(tag) if tag.local_name().as_ref() == b"cellXfs" => inside = true,
            Event::End(tag) if tag.local_name().as_ref() == b"cellXfs" => inside = false,
            Event::Start(tag) | Event::Empty(tag) if inside => match tag.local_name().as_ref() {
                b"xf" => index += 1,
                b"alignment" if index > 0 && enabled(&tag, b"shrinkToFit") => {
                    css.push_str(&format!(".hcd-xs-{}{{--hcd-shrink-to-fit:1}}\n", index - 1));
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(css)
}

fn enabled(element: &BytesStart<'_>, key: &[u8]) -> bool {
    element.attributes().flatten().any(|attr| {
        attr.key.local_name().as_ref() == key && matches!(attr.value.as_ref(), b"1" | b"true")
    })
}

fn presentation(source: &Path) -> anyhow::Result<String> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(source)?)?;
    let names: Vec<_> = archive
        .file_names()
        .filter(|name| {
            name.strip_prefix("ppt/slides/slide")
                .and_then(|n| n.strip_suffix(".xml"))
                .is_some_and(|digits| {
                    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
                })
        })
        .map(str::to_owned)
        .collect();
    let mut css = String::new();
    for name in names {
        css.push_str(&slide_styles(&name, &read_part(&mut archive, &name)?)?);
    }
    Ok(css)
}

fn slide_styles(part: &str, xml: &str) -> anyhow::Result<String> {
    let mut reader = Reader::from_str(xml);
    let (mut picture, mut id, mut css) = (false, None, String::new());
    loop {
        match reader.read_event()? {
            Event::Start(tag) if tag.local_name().as_ref() == b"pic" => {
                picture = true;
                id = None;
            }
            Event::End(tag) if tag.local_name().as_ref() == b"pic" => picture = false,
            Event::Start(tag) | Event::Empty(tag) if picture => match tag.local_name().as_ref() {
                b"cNvPr" => id = number(&tag, b"id"),
                b"srcRect" => {
                    if let Some(id) = id {
                        css.push_str(&crop_style(part, id, &tag));
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(css)
}

fn crop_style(part: &str, id: f64, tag: &BytesStart<'_>) -> String {
    let [left, top, right, bottom] =
        [b"l", b"t", b"r", b"b"].map(|key| number(tag, key).unwrap_or(0.0) / PERCENT_UNIT);
    let (width, height) = (1.0 - left - right, 1.0 - top - bottom);
    if width <= 0.0 || height <= 0.0 {
        return String::new();
    }
    format!(
        "[data-hcd-source-part=\"{part}\"][data-hcd-picture-id=\"{id}\"]>img{{max-width:none;object-fit:fill;width:{:.6}%;height:{:.6}%;margin-left:{:.6}%;position:relative;top:{:.6}%}}\n",
        100.0 / width, 100.0 / height, -100.0 * left / width, -100.0 * top / height
    )
}
