use std::io::{Read, Write};
use std::sync::Arc;

use super::{
    assets::{Asset, AssetCache},
    error::GatewayError,
    json_scan::{self, ImageFormat, ImageSlot},
    media,
    spool::Spool,
    Runtime,
};

pub(super) enum Output {
    Request(Spool),
    Prepared(Asset),
}

struct Replacement {
    bytes: Vec<u8>,
    copy_fields: Vec<std::ops::Range<u64>>,
}

pub(super) async fn prepare(
    runtime: &Arc<Runtime>,
    body: Spool,
    (path, token): (&str, &str),
) -> Result<Output, GatewayError> {
    let path = path.to_owned();
    let (body, scan) = super::work::blocking(runtime, move || {
        let scan = if path.ends_with("/images/input/prepare") || body.may_contain_images()? {
            json_scan::scan(body.reader()?, &path)?
        } else {
            Default::default()
        };
        Ok::<_, GatewayError>((body, scan))
    })
    .await?;
    if scan.images.is_empty() {
        return Ok(Output::Request(body));
    }
    let _permit = runtime
        .images
        .acquire()
        .await
        .map_err(|_| GatewayError::unavailable("Image queue closed"))?;
    prepare_images(runtime, body, (scan, token)).await
}

async fn prepare_images(
    runtime: &Arc<Runtime>,
    body: Spool,
    (scan, token): (json_scan::Scan, &str),
) -> Result<Output, GatewayError> {
    let mut replacements = Vec::new();
    let mut body = body;
    for slot in scan.images {
        let (returned, slot, hash) = super::work::blocking(runtime, move || {
            let hash = media::encoded_digest(&body, &slot)?;
            Ok::<_, GatewayError>((body, slot, hash))
        })
        .await?;
        body = returned;
        let key = AssetCache::key(token, &hash);
        let _same_asset = runtime.asset_locks[usize::from(key[0]) % runtime.asset_locks.len()]
            .lock()
            .await;
        let asset = match runtime.cache.get(&key) {
            Some(asset) => {
                validate_cached(&slot, &asset)?;
                asset
            }
            None => {
                let (returned, slot_returned, decoded) =
                    super::work::blocking(runtime, move || {
                        let prepared = media::decode(&body, &slot, hash)?;
                        Ok::<_, GatewayError>((body, slot, prepared))
                    })
                    .await?;
                body = returned;
                let asset = runtime.upload((token, &scan.model), decoded).await?;
                if scan.operation == "upload" {
                    return Ok(Output::Prepared(asset));
                }
                replacements.push((
                    slot_returned.replace.clone(),
                    replacement(&slot_returned, &asset)?,
                ));
                continue;
            }
        };
        if scan.operation == "upload" {
            return Ok(Output::Prepared(asset));
        }
        replacements.push((slot.replace.clone(), replacement(&slot, &asset)?));
    }
    let rewritten = super::work::blocking(runtime, move || apply(body, replacements)).await?;
    Ok(Output::Request(rewritten))
}

fn validate_cached(slot: &ImageSlot, asset: &Asset) -> Result<(), GatewayError> {
    if slot
        .mime
        .as_deref()
        .is_some_and(|mime| mime != asset.mime_type)
    {
        return Err(GatewayError::input(
            "Image MIME type does not match cached bytes",
        ));
    }
    Ok(())
}

fn replacement(slot: &ImageSlot, asset: &Asset) -> Result<Replacement, GatewayError> {
    let value = match slot.format {
        ImageFormat::Messages => {
            serde_json::json!({"type":"url", "url":format!("iyw-image:{}", asset.image_id)})
        }
        ImageFormat::Responses | ImageFormat::Chat => {
            serde_json::json!(format!("iyw-image:{}", asset.image_id))
        }
        ImageFormat::Edit => serde_json::json!(format!("iyw-image:{}", asset.image_id)),
    };
    let mut bytes = serde_json::to_vec(&value)
        .map_err(|_| GatewayError::unavailable("Image reference encoding failed"))?;
    let copy_fields = if matches!(slot.format, ImageFormat::Messages) {
        slot.source_extra.clone()
    } else {
        Vec::new()
    };
    if !copy_fields.is_empty() {
        bytes.pop();
    }
    Ok(Replacement { bytes, copy_fields })
}

fn apply(
    body: Spool,
    mut replacements: Vec<(std::ops::Range<u64>, Replacement)>,
) -> Result<Spool, GatewayError> {
    replacements.sort_by_key(|(range, _)| range.start);
    let mut reader = body.reader()?;
    let mut file = super::spool_budget::SpoolWriter::new()?;
    let mut offset = 0;
    for (range, value) in replacements {
        if range.start < offset {
            return Err(GatewayError::input("Overlapping image fields"));
        }
        std::io::copy(&mut reader.by_ref().take(range.start - offset), &mut file)?;
        std::io::copy(
            &mut reader.by_ref().take(range.end - range.start),
            &mut std::io::sink(),
        )?;
        file.write_all(&value.bytes)?;
        for field in &value.copy_fields {
            file.write_all(b",")?;
            let mut source = body.reader_at(field.start)?.take(field.end - field.start);
            std::io::copy(&mut source, &mut file)?;
        }
        if !value.copy_fields.is_empty() {
            file.write_all(b"}")?;
            // try_clone 的文件句柄共享游标，复制额外字段后恢复主读取位置。
            reader = body.reader_at(range.end)?;
        }
        offset = range.end;
    }
    std::io::copy(&mut reader, &mut file)?;
    Spool::from_file(file)
}
