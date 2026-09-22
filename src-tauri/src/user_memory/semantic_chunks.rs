use qdrant_edge::PointId;

use super::index_types::IndexItem;

// UTF-8 字节预算保守覆盖两家模型的文本限制，版本进入索引身份。
pub(super) const CHUNK_VERSION: &str = "utf8-768-v1";
const CHUNK_BYTES: usize = 768;

pub(super) struct MemoryChunk {
    pub id: PointId,
    pub memory_id: String,
    pub digest: String,
    pub text: String,
}

pub(super) fn texts(content: &str) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut remaining = content;
    while !remaining.is_empty() {
        let mut end = remaining.len().min(CHUNK_BYTES);
        while !remaining.is_char_boundary(end) {
            end -= 1;
        }
        let (chunk, rest) = remaining.split_at(end);
        if !chunk.trim().is_empty() {
            chunks.push(chunk);
        }
        remaining = rest;
    }
    chunks
}

pub(super) fn chunks(item: &IndexItem) -> Vec<MemoryChunk> {
    texts(&item.content)
        .into_iter()
        .enumerate()
        .map(|(offset, text)| {
            let fingerprint = super::helpers::hash_parts(&[
                item.id.as_bytes(),
                item.content_digest.as_bytes(),
                &offset.to_le_bytes(),
            ]);
            MemoryChunk {
                id: PointId::Uuid(uuid::Uuid::parse_str(&fingerprint[..32]).expect("valid digest")),
                memory_id: item.id.clone(),
                digest: item.content_digest.clone(),
                text: text.into(),
            }
        })
        .collect()
}

pub(super) fn point_ids(item: &IndexItem) -> Vec<PointId> {
    chunks(item).into_iter().map(|chunk| chunk.id).collect()
}
