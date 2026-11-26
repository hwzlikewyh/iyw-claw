use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_LOGO_BYTES: u64 = 5 * 1024 * 1024;
const MAX_LOGO_SIDE: u32 = 2048;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Configuration {
    #[serde(default = "default_id")]
    brand_id: String,
    display_name: String,
    #[serde(default)]
    release_id: String,
    #[serde(default = "default_channel")]
    promotion_channel: String,
    logo_file: Option<String>,
}

pub struct Brand {
    pub id: String,
    pub name: String,
    pub identifier: String,
    pub scheme: String,
    pub credential_service: String,
    pub release_id: String,
    pub promotion_channel: String,
    pub logo: Option<Vec<u8>>,
}

fn default_id() -> String { "iyw".to_string() }
fn default_channel() -> String { "stable".to_string() }

impl Default for Brand {
    fn default() -> Self {
        Self { id: default_id(), name: "原助理".into(), identifier: "app.iywclaw".into(),
            scheme: "iyw-assistant".into(), credential_service: "iyw-claw".into(),
            release_id: String::new(), promotion_channel: default_channel(), logo: None }
    }
}

pub fn configuration_path(executable: &Path) -> Option<PathBuf> {
    for directory in executable.ancestors().skip(1) {
        if directory.extension().is_some_and(|extension| extension == "app") {
            return Some(PathBuf::from(format!("{}.branding", directory.display())).join("branding.json"));
        }
        if directory.file_name().is_some_and(|name| name == "app") {
            return directory.parent().map(|root| root.join("config/branding.json"));
        }
    }
    None
}

pub fn load() -> Result<Brand, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let Some(path) = configuration_path(&executable) else { return Ok(Brand::default()); };
    match std::fs::metadata(&path) {
        Ok(_) => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Brand::default()),
        Err(error) => return Err(format!("读取品牌配置状态失败：{error}")),
    }
    let bytes = read_limited(&path, MAX_CONFIG_BYTES)?;
    let input: Configuration = serde_json::from_slice(&bytes)
        .map_err(|error| format!("品牌配置格式错误：{error}"))?;
    validate(&input)?;
    let custom = input.brand_id != "iyw";
    let suffix = if custom { format!("-{}", input.brand_id) } else { String::new() };
    let identifier = if custom { format!("app.iywclaw.{}", input.brand_id) } else { "app.iywclaw".into() };
    let logo = input.logo_file.as_deref().map(|file| load_logo(&path, file)).transpose()?;
    Ok(Brand { id: input.brand_id, name: input.display_name, identifier,
        scheme: format!("iyw-assistant{suffix}"), credential_service: format!("iyw-claw{suffix}"),
        release_id: input.release_id, promotion_channel: input.promotion_channel, logo })
}

fn validate(input: &Configuration) -> Result<(), String> {
    if !matches!(input.promotion_channel.as_str(), "stable" | "beta") {
        return Err("晋级后的更新通道无效".into());
    }
    let id = input.brand_id.as_bytes();
    if !(2..=32).contains(&id.len()) || !id[0].is_ascii_lowercase()
        || !id.iter().all(|value| value.is_ascii_lowercase() || value.is_ascii_digit()) {
        return Err("品牌标识无效".into());
    }
    if input.display_name.trim().is_empty() || input.display_name.chars().count() > 64
        || input.display_name.chars().any(char::is_control) {
        return Err("品牌名称无效".into());
    }
    if input.release_id.len() > 19 || !input.release_id.bytes().all(|value| value.is_ascii_digit()) {
        return Err("品牌候选发布编号无效".into());
    }
    Ok(())
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|error| format!("读取品牌文件失败：{error}"))?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit { return Err("品牌文件超过大小限制".into()); }
    Ok(bytes)
}

fn load_logo(configuration: &Path, relative: &str) -> Result<Vec<u8>, String> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty() || relative.components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err("Logo 必须使用配置目录中的相对路径".into());
    }
    let bytes = read_limited(&configuration.parent().unwrap().join(relative), MAX_LOGO_BYTES)?;
    if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") { return Err("品牌 Logo 必须为 PNG".into()); }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if width == 0 || height == 0 || width > MAX_LOGO_SIDE || height > MAX_LOGO_SIDE {
        return Err("品牌 Logo 宽高超过限制".into());
    }
    image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .map_err(|error| format!("品牌 Logo 无效：{error}"))?;
    Ok(bytes)
}
