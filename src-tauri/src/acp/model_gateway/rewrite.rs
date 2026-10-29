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
    let scan_path = path.clone();
    let (body, scan) = super::work::blocking(runtime, move || {
        let scan = if scan_path.ends_with("/images/input/prepare") || body.may_contain_images()? {
            json_scan::scan(body.reader()?, &scan_path)?
        } else {
            Default::default()
        };
        Ok::<_, GatewayError>((body, scan))
    })
    .await?;
    if scan.images.is_empty() {
        return Ok(Output::Request(body));
    }
    let ids = missing_references(runtime, token, &scan);
    let urls = scan.operation != "upload"
        && runtime
            .local_url_transport(token, (&scan.model, &path), &ids)
            .await;
    let inline = scan.images.iter().any(|image| image.managed_id.is_none());
    if !urls && !inline {
        return Ok(Output::Request(body));
    }
    let _permit = if inline {
        Some(
            runtime
                .images
                .acquire()
                .await
                .map_err(|_| GatewayError::unavailable("Image queue closed"))?,
        )
    } else {
        None
    };
    prepare_images(runtime, body, (scan, token, urls)).await
}

async fn prepare_images(
    runtime: &Arc<Runtime>,
    body: Spool,
    (scan, token, urls): (json_scan::Scan, &str, bool),
) -> Result<Output, GatewayError> {
    let mut replacements = Vec::new();
    let mut body = body;
    let (original_bytes, image_count) = (body.len(), scan.images.len());
    let mut prepared_images = 0;
    for slot in scan.images {
        if let Some(id) = &slot.managed_id {
            if urls {
                if let Some(asset) = runtime.cache.get_reference(token, id) {
                    push_replacement(&mut replacements, &slot, (&asset, true))?;
                    prepared_images += 1;
                }
            }
            continue;
        }
        let (returned, slot, asset) =
            prepare_inline(runtime, body, (slot, token, &scan.model)).await?;
        body = returned;
        if scan.operation == "upload" {
            return Ok(Output::Prepared(asset));
        }
        push_replacement(&mut replacements, &slot, (&asset, urls))?;
        prepared_images += 1;
    }
    if replacements.is_empty() {
        return Ok(Output::Request(body));
    }
    let rewritten = super::work::blocking(runtime, move || apply(body, replacements)).await?;
    tracing::debug!(target: "model_gateway", model=scan.model, image_count, prepared_images,
        deferred_images=image_count-prepared_images, url_transport=urls,
        original_bytes, final_bytes=rewritten.len(), "local image request preparation complete");
    Ok(Output::Request(rewritten))
}

async fn prepare_inline(
    runtime: &Runtime,
    body: Spool,
    (slot, token, model): (ImageSlot, &str, &str),
) -> Result<(Spool, ImageSlot, Asset), GatewayError> {
    let (body, slot, hash) = super::work::blocking(runtime, move || {
        let hash = media::encoded_digest(&body, &slot)?;
        Ok::<_, GatewayError>((body, slot, hash))
    })
    .await?;
    let key = AssetCache::key(token, &hash);
    let _same_asset = runtime.asset_locks[usize::from(key[0]) % runtime.asset_locks.len()]
        .lock()
        .await;
    if let Some(asset) = runtime.cache.get(&key) {
        validate_cached(&slot, &asset)?;
        return Ok((body, slot, asset));
    }
    let (body, slot, decoded) = super::work::blocking(runtime, move || {
        let prepared = media::decode(&body, &slot, hash)?;
        Ok::<_, GatewayError>((body, slot, prepared))
    })
    .await?;
    let asset = runtime.upload((token, model), decoded).await?;
    Ok((body, slot, asset))
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

fn replacement(slot: &ImageSlot, asset: &Asset, urls: bool) -> Result<Replacement, GatewayError> {
    let reference = if urls {
        asset.image_url.clone()
    } else {
        format!("iyw-image:{}", asset.image_id)
    };
    let value = match slot.format {
        ImageFormat::Messages => {
            serde_json::json!({"type":"url", "url":reference})
        }
        ImageFormat::Responses | ImageFormat::Chat => {
            serde_json::json!(reference)
        }
        ImageFormat::Edit => serde_json::json!(reference),
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

fn missing_references(runtime: &Runtime, token: &str, scan: &json_scan::Scan) -> Vec<String> {
    let mut ids = Vec::new();
    for slot in &scan.images {
        if let Some(id) = &slot.managed_id {
            if runtime.cache.get_reference(token, id).is_none() && !ids.contains(id) {
                ids.push(id.clone());
            }
        }
    }
    ids
}

fn push_replacement(
    replacements: &mut Vec<(std::ops::Range<u64>, Replacement)>,
    slot: &ImageSlot,
    (asset, urls): (&Asset, bool),
) -> Result<(), GatewayError> {
    replacements.push((slot.replace.clone(), replacement(slot, asset, urls)?));
    if let Some(range) = &slot.rename {
        replacements.push((
            range.clone(),
            Replacement {
                bytes: b"\"image_url\"".to_vec(),
                copy_fields: Vec::new(),
            },
        ));
    }
    Ok(())
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
