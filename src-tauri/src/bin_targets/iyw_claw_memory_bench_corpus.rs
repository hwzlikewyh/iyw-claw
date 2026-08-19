use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Clone, Serialize)]
pub(crate) struct SyntheticMemory {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) content: String,
    pub(crate) content_digest: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) scope: Scope,
    pub(crate) sensitive: bool,
    pub(crate) source_revision: String,
    pub(crate) valid_from: String,
    pub(crate) valid_to: Option<String>,
    pub(crate) conflict_group: Option<String>,
    pub(crate) relation_ids: Vec<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct Scope {
    pub(crate) r#type: String,
    pub(crate) key: String,
}

pub(crate) fn generate_corpus(size: usize, seed: u64) -> Vec<SyntheticMemory> {
    let mut rng = XorShift64::new(seed);
    (0..size)
        .map(|index| make_item(index, size, seed, &mut rng))
        .collect()
}

pub(crate) fn serialize_corpus(corpus: &[SyntheticMemory]) -> Result<String, String> {
    let mut jsonl = String::new();
    for item in corpus {
        let row = serde_json::to_string(item).map_err(|error| error.to_string())?;
        jsonl.push_str(&row);
        jsonl.push('\n');
    }
    Ok(jsonl)
}

pub(crate) fn digest(value: &str) -> String {
    digest_bytes(value.as_bytes())
}

pub(crate) fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn make_item(index: usize, size: usize, seed: u64, rng: &mut XorShift64) -> SyntheticMemory {
    let bucket = index % 10;
    let scope = scope_for(index);
    let id = format!("mem://synthetic/v1/{seed:016x}/{index:05}", seed = seed);
    let topic = TOPICS[rng.index(TOPICS.len())];
    let content = content_for(bucket, topic, index);
    SyntheticMemory {
        content_digest: digest(&content),
        aliases: aliases_for(bucket, topic, index),
        kind: kind_for(bucket).to_string(),
        source_revision: format!("r{}", 1 + index % 7),
        valid_from: format!("202{}-{:02}-01T00:00:00Z", index % 4 + 2, index % 12 + 1),
        valid_to: valid_to_for(bucket, index),
        conflict_group: conflict_group(bucket, index),
        relation_ids: relation_ids(index, size, &id),
        id,
        content,
        scope,
        sensitive: bucket == 8,
    }
}

fn scope_for(index: usize) -> Scope {
    if index % 10 == 7 {
        Scope {
            r#type: "workspace".to_string(),
            key: format!("ws-{:02}", index % 17),
        }
    } else {
        Scope {
            r#type: "global".to_string(),
            key: String::new(),
        }
    }
}

fn kind_for(bucket: usize) -> &'static str {
    match bucket {
        0 => "exact",
        1 => "alias",
        2 => "cjk",
        3 => "short",
        4 => "temporal",
        5 => "update",
        6 => "conflict",
        7 => "scope",
        8 => "sensitive",
        _ => "noise",
    }
}

fn content_for(bucket: usize, topic: &str, index: usize) -> String {
    match bucket {
        2 => format!("\u{9879}\u{76ee}{topic}\u{72b6}\u{6001}{index}"),
        3 => format!("{topic} queue owns short code K{}.", index % 97),
        4 => format!("{topic} schedule is valid after phase {}.", index % 12),
        5 => format!("{topic} current owner revision is {}.", index % 7),
        6 => format!(
            "{topic} review has synthetic conflicting evidence {}.",
            index / 10
        ),
        8 => format!("{topic} restricted synthetic marker {index}."),
        _ => format!("{topic} synthetic memory item {index}."),
    }
}

fn aliases_for(bucket: usize, topic: &str, index: usize) -> Vec<String> {
    match bucket {
        1 => vec![format!("{topic}-alias"), format!("{topic} board")],
        2 => vec![
            format!("\u{9879}\u{76ee}{topic}"),
            "\u{4e2d}\u{6587}\u{72b6}\u{6001}".to_string(),
        ],
        3 => vec![format!("K{}", index % 97)],
        _ => vec![format!("{topic}-{index}")],
    }
}

fn valid_to_for(bucket: usize, index: usize) -> Option<String> {
    if bucket == 4 && index % 3 == 0 {
        Some("2030-01-01T00:00:00Z".to_string())
    } else {
        None
    }
}

fn conflict_group(bucket: usize, index: usize) -> Option<String> {
    (bucket == 6).then(|| format!("conflict-{:05}", index / 20))
}

fn relation_ids(index: usize, size: usize, id: &str) -> Vec<String> {
    if index == 0 || index + 1 == size {
        return Vec::new();
    }
    vec![id.replace(&format!("/{index:05}"), &format!("/{:05}", index - 1))]
}

struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn index(&mut self, len: usize) -> usize {
        (self.next() as usize) % len
    }
}

const TOPICS: [&str; 8] = [
    "Cedar", "Juniper", "Lumen", "Nova", "Orbit", "Summit", "Atlas", "Harbor",
];
