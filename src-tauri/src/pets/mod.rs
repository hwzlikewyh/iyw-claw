//! Filesystem-backed pet repository.
//!
//! All access is synchronous I/O wrapped in `tokio::task::spawn_blocking` by
//! callers when needed. The repository reads from / writes to
//! `paths::iyw_claw_pets_root()` and is **decoupled from Tauri** so the same
//! routines back the desktop and standalone-server runtimes.
//!
//! Format mirrors Codex `/pet`:
//!
//! ```text
//! <pets-root>/<pet-id>/
//!     pet.json
//!     spritesheet.webp
//! ```
//!
//! Where `pet.json` carries `{ id, displayName, description?, spritesheetPath }`
//! and the spritesheet is a 1536×1872 RGBA WebP (PNG also accepted).

pub mod codex_import;
pub mod marketplace;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::{ImageFormat, ImageReader};

use crate::app_error::AppCommandError;
use crate::models::pet::{
    NewPetInput, PetDetail, PetManifest, PetMetaPatch, PetSpriteAsset, PetSummary,
    PET_MANIFEST_FILENAME, SPRITESHEET_FILENAME, SPRITE_FRAME_HEIGHT, SPRITE_GRID_ROWS,
    SPRITE_SHEET_WIDTH,
};
use crate::paths::iyw_claw_pets_root;

/// Smallest plausible sprite-sheet payload; rejecting tiny inputs early
/// avoids decoding random files.
const MIN_SPRITE_BYTES: usize = 1024;
/// Cap raw sprite uploads at 16 MiB. A correctly-encoded 1536×1872 WebP is
/// usually well under 1 MiB; this is purely a guardrail.
const MAX_SPRITE_BYTES: usize = 16 * 1024 * 1024;
const MAX_SPRITE_ROWS: u32 = 32;

/// Detected sprite-sheet container.
#[derive(Debug, Clone, Copy)]
pub enum SpriteFormat {
    Png,
    Webp,
}

impl SpriteFormat {
    pub const fn mime(self) -> &'static str {
        match self {
            SpriteFormat::Png => "image/png",
            SpriteFormat::Webp => "image/webp",
        }
    }

    pub const fn filename(self) -> &'static str {
        // We always *store* under the canonical Codex name so directories are
        // round-trippable. PNG uploads are renamed at write time.
        SPRITESHEET_FILENAME
    }
}

/// Decode the image header just enough to verify the sprite-sheet contract.
/// Returns the detected format on success.
pub fn validate_spritesheet(bytes: &[u8]) -> Result<SpriteFormat, AppCommandError> {
    if bytes.len() < MIN_SPRITE_BYTES {
        return Err(AppCommandError::invalid_input(
            "Spritesheet payload is too small to be valid.",
        ));
    }
    if bytes.len() > MAX_SPRITE_BYTES {
        return Err(AppCommandError::invalid_input(format!(
            "Spritesheet payload exceeds {} MiB cap.",
            MAX_SPRITE_BYTES / (1024 * 1024)
        )));
    }

    let cursor = std::io::Cursor::new(bytes);
    let reader = ImageReader::new(cursor)
        .with_guessed_format()
        .map_err(|e| AppCommandError::invalid_input(format!("Cannot read sprite header: {e}")))?;
    let format = reader.format().ok_or_else(|| {
        AppCommandError::invalid_input("Spritesheet must be a PNG or WebP image.")
    })?;
    let detected = match format {
        ImageFormat::Png => SpriteFormat::Png,
        ImageFormat::WebP => SpriteFormat::Webp,
        _ => {
            return Err(AppCommandError::invalid_input(
                "Spritesheet must be a PNG or WebP image.",
            ));
        }
    };

    let img = reader
        .decode()
        .map_err(|e| AppCommandError::invalid_input(format!("Cannot decode sprite: {e}")))?;
    check_sprite_dimensions(img.width(), img.height())?;
    if !img.color().has_alpha() {
        return Err(AppCommandError::invalid_input(
            "Spritesheet must contain an alpha channel (transparent background).",
        ));
    }

    Ok(detected)
}

fn check_sprite_dimensions(width: u32, height: u32) -> Result<(), AppCommandError> {
    if width != SPRITE_SHEET_WIDTH {
        return Err(AppCommandError::invalid_input(format!(
            "Spritesheet must be {SPRITE_SHEET_WIDTH}px wide; got width {width}."
        )));
    }
    if height == 0 || !height.is_multiple_of(SPRITE_FRAME_HEIGHT) {
        return Err(AppCommandError::invalid_input(format!(
            "Spritesheet height must be a whole multiple of {SPRITE_FRAME_HEIGHT}px; got {height}."
        )));
    }
    let rows = height / SPRITE_FRAME_HEIGHT;
    if rows < SPRITE_GRID_ROWS {
        return Err(AppCommandError::invalid_input(format!(
            "Spritesheet must have at least {SPRITE_GRID_ROWS} rows; got {rows}."
        )));
    }
    if rows > MAX_SPRITE_ROWS {
        return Err(AppCommandError::invalid_input(format!(
            "Spritesheet has too many rows ({rows}); the maximum is {MAX_SPRITE_ROWS}."
        )));
    }
    Ok(())
}

/// Slug-validate a pet id. Returns `Err` on invalid input. Defense in depth:
/// the frontend slugifies but we must independently reject malformed ids
/// before they touch the filesystem.
pub fn validate_pet_id(id: &str) -> Result<(), AppCommandError> {
    if id.is_empty() {
        return Err(AppCommandError::invalid_input("Pet id is required."));
    }
    if id.len() > 64 {
        return Err(AppCommandError::invalid_input(
            "Pet id must be at most 64 characters.",
        ));
    }
    let valid = id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !valid {
        return Err(AppCommandError::invalid_input(
            "Pet id may only contain lowercase letters, digits, '-' and '_'.",
        ));
    }
    if id.starts_with('-') || id.ends_with('-') || id.starts_with('.') {
        return Err(AppCommandError::invalid_input(
            "Pet id cannot start with '.' or '-' / end with '-'.",
        ));
    }
    Ok(())
}

fn pet_dir(id: &str) -> Result<PathBuf, AppCommandError> {
    validate_pet_id(id)?;
    Ok(iyw_claw_pets_root().join(id))
}

fn ensure_pets_root() -> Result<PathBuf, AppCommandError> {
    ensure_pets_root_or_create()
}

/// Public alias used by `codex_import` to share the same create-if-missing
/// behaviour without exposing the rest of the module's private helpers.
pub(crate) fn ensure_pets_root_or_create() -> Result<PathBuf, AppCommandError> {
    let root = iyw_claw_pets_root();
    if !root.exists() {
        fs::create_dir_all(&root).map_err(AppCommandError::io)?;
    }
    Ok(root)
}

/// Snapshot of pet ids currently on disk. Exposed for `codex_import` to
/// detect collisions before copying.
pub(crate) fn list_existing_ids() -> Result<std::collections::HashSet<String>, AppCommandError> {
    let root = iyw_claw_pets_root();
    if !root.is_dir() {
        return Ok(std::collections::HashSet::new());
    }
    let mut out = std::collections::HashSet::new();
    for entry in fs::read_dir(&root).map_err(AppCommandError::io)?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            out.insert(name.to_string());
        }
    }
    Ok(out)
}

fn read_manifest(dir: &Path) -> Result<PetManifest, AppCommandError> {
    let manifest_path = dir.join(PET_MANIFEST_FILENAME);
    let raw = fs::read_to_string(&manifest_path).map_err(AppCommandError::io)?;
    serde_json::from_str::<PetManifest>(&raw).map_err(|e| {
        AppCommandError::invalid_input(format!(
            "Malformed pet manifest at {}: {e}",
            manifest_path.display()
        ))
    })
}

fn write_manifest_atomic(dir: &Path, manifest: &PetManifest) -> Result<(), AppCommandError> {
    let final_path = dir.join(PET_MANIFEST_FILENAME);
    let tmp_path = dir.join(format!("{PET_MANIFEST_FILENAME}.tmp"));
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| AppCommandError::io_error(format!("Failed to serialize pet manifest: {e}")))?;
    {
        let mut f = fs::File::create(&tmp_path).map_err(AppCommandError::io)?;
        f.write_all(json.as_bytes()).map_err(AppCommandError::io)?;
        f.write_all(b"\n").map_err(AppCommandError::io)?;
        f.sync_all().map_err(AppCommandError::io)?;
    }
    fs::rename(&tmp_path, &final_path).map_err(AppCommandError::io)?;
    Ok(())
}

fn write_spritesheet_atomic(dir: &Path, bytes: &[u8]) -> Result<(), AppCommandError> {
    let final_path = dir.join(SPRITESHEET_FILENAME);
    let tmp_path = dir.join(format!("{SPRITESHEET_FILENAME}.tmp"));
    {
        let mut f = fs::File::create(&tmp_path).map_err(AppCommandError::io)?;
        f.write_all(bytes).map_err(AppCommandError::io)?;
        f.sync_all().map_err(AppCommandError::io)?;
    }
    fs::rename(&tmp_path, &final_path).map_err(AppCommandError::io)?;
    Ok(())
}

fn decode_base64_payload(b64: &str) -> Result<Vec<u8>, AppCommandError> {
    BASE64
        .decode(b64.as_bytes())
        .map_err(|e| AppCommandError::invalid_input(format!("Invalid base64 payload: {e}")))
}

/// Enumerate well-formed pets in `pets-root`. Bad entries (missing files,
/// malformed manifests) are skipped silently so a single corrupt directory
/// cannot break the picker. The bad entries get logged for diagnosis.
pub fn list_pets() -> Result<Vec<PetSummary>, AppCommandError> {
    let root = iyw_claw_pets_root();
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = match fs::read_dir(&root) {
        Ok(it) => it,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(AppCommandError::io(err)),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest = match read_manifest(&path) {
            Ok(m) => m,
            Err(err) => {
                tracing::warn!("[Pets] skipping {}: {}", path.display(), err.message);
                continue;
            }
        };
        let spritesheet = path.join(SPRITESHEET_FILENAME);
        if !spritesheet.exists() {
            tracing::warn!("[Pets] skipping {}: spritesheet missing", path.display());
            continue;
        }
        out.push(PetSummary {
            id: manifest.id,
            display_name: manifest.display_name,
            description: manifest.description,
            spritesheet_path: spritesheet,
        });
    }
    out.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
    });
    Ok(out)
}

pub fn get_pet(id: &str) -> Result<PetDetail, AppCommandError> {
    let dir = pet_dir(id)?;
    if !dir.is_dir() {
        return Err(AppCommandError::not_found(format!("Pet '{id}' not found.")));
    }
    let manifest = read_manifest(&dir)?;
    let spritesheet = dir.join(SPRITESHEET_FILENAME);
    if !spritesheet.exists() {
        return Err(AppCommandError::not_found(format!(
            "Pet '{id}' has no spritesheet on disk."
        )));
    }
    Ok(PetDetail {
        id: manifest.id,
        display_name: manifest.display_name,
        description: manifest.description,
        spritesheet_path: spritesheet,
    })
}

pub fn read_pet_spritesheet(id: &str) -> Result<PetSpriteAsset, AppCommandError> {
    let dir = pet_dir(id)?;
    let spritesheet = dir.join(SPRITESHEET_FILENAME);
    let bytes = fs::read(&spritesheet).map_err(AppCommandError::io)?;
    let mime = sniff_mime(&bytes);
    Ok(PetSpriteAsset {
        mime: mime.to_string(),
        data_base64: BASE64.encode(&bytes),
    })
}

fn sniff_mime(bytes: &[u8]) -> &'static str {
    // Header sniff is enough — `validate_spritesheet` already ran on write,
    // so on-disk bytes are guaranteed PNG or WebP. Fallback to webp on
    // ambiguity since that's the canonical Codex format.
    if bytes.len() >= 8 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" {
        return "image/png";
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return "image/webp";
    }
    "image/webp"
}

pub fn add_pet(input: NewPetInput) -> Result<PetSummary, AppCommandError> {
    validate_pet_id(&input.id)?;
    if input.display_name.trim().is_empty() {
        return Err(AppCommandError::invalid_input("Display name is required."));
    }

    let bytes = decode_base64_payload(&input.spritesheet_base64)?;
    let _format = validate_spritesheet(&bytes)?;

    let root = ensure_pets_root()?;
    let target = root.join(&input.id);
    if target.exists() {
        return Err(AppCommandError::already_exists(format!(
            "A pet with id '{}' already exists.",
            input.id
        )));
    }

    // Stage in a sibling tmp dir, then rename atomically so a crashed
    // mid-write never leaves a half-built pet on disk.
    let tmp_dir = root.join(format!("{}.import.tmp", input.id));
    if tmp_dir.exists() {
        // Leftover from a previous failure — purge it.
        let _ = fs::remove_dir_all(&tmp_dir);
    }
    fs::create_dir_all(&tmp_dir).map_err(AppCommandError::io)?;

    let manifest = PetManifest {
        id: input.id.clone(),
        display_name: input.display_name.trim().to_string(),
        description: input
            .description
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        spritesheet_path: SPRITESHEET_FILENAME.to_string(),
        extra: serde_json::Map::new(),
    };
    if let Err(err) = write_manifest_atomic(&tmp_dir, &manifest) {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err(err);
    }
    if let Err(err) = write_spritesheet_atomic(&tmp_dir, &bytes) {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err(err);
    }

    if let Err(err) = fs::rename(&tmp_dir, &target) {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err(AppCommandError::io(err));
    }

    Ok(PetSummary {
        id: manifest.id,
        display_name: manifest.display_name,
        description: manifest.description,
        spritesheet_path: target.join(SPRITESHEET_FILENAME),
    })
}

pub fn update_pet_meta(id: &str, patch: PetMetaPatch) -> Result<PetSummary, AppCommandError> {
    let dir = pet_dir(id)?;
    if !dir.is_dir() {
        return Err(AppCommandError::not_found(format!("Pet '{id}' not found.")));
    }
    let mut manifest = read_manifest(&dir)?;

    if let Some(name) = patch.display_name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(AppCommandError::invalid_input(
                "Display name cannot be blank.",
            ));
        }
        manifest.display_name = trimmed.to_string();
    }
    if let Some(desc) = patch.description {
        manifest.description = desc.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    }

    write_manifest_atomic(&dir, &manifest)?;
    Ok(PetSummary {
        id: manifest.id,
        display_name: manifest.display_name,
        description: manifest.description,
        spritesheet_path: dir.join(SPRITESHEET_FILENAME),
    })
}

pub fn replace_pet_sprite(id: &str, spritesheet_base64: &str) -> Result<(), AppCommandError> {
    let dir = pet_dir(id)?;
    if !dir.is_dir() {
        return Err(AppCommandError::not_found(format!("Pet '{id}' not found.")));
    }
    let bytes = decode_base64_payload(spritesheet_base64)?;
    validate_spritesheet(&bytes)?;
    write_spritesheet_atomic(&dir, &bytes)
}

pub fn delete_pet(id: &str) -> Result<(), AppCommandError> {
    let dir = pet_dir(id)?;
    if !dir.is_dir() {
        // Idempotent delete — already gone is success.
        return Ok(());
    }
    fs::remove_dir_all(&dir).map_err(AppCommandError::io)
}

