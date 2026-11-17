//! 内置OfficeCLI-rust引擎、技能同步与市场覆盖兼容接口。

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};

use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use serde::Serialize;
use tokio::sync::{watch, Mutex};

use crate::acp::types::AgentSkillScope;
use crate::app_error::AppCommandError;

use crate::commands::acp::{
    preferred_scope_skill_dir, remove_skill_entry, scoped_skill_dirs, validate_skill_id,
};
use crate::commands::experts::{
    central_experts_dir, classify_link, managed_copy_is_owned, managed_link_is_owned,
    path_is_symlink, read_link_target, reconcile_managed_link_entry, reconcile_managed_link_paths,
    ExpertInstallStatus, ExpertLinkState, LinkOp, LinkOpResult, ManagedLinkChange,
    ManagedLinkEntryError,
};
use crate::commands::folders::resolve_tree_path;
use crate::models::agent::AgentType;

use crate::web::event_bridge::EventEmitter;

// ─── Error type ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, thiserror::Error)]
pub enum OfficeToolsError {
    #[error("officecli is not installed")]
    NotInstalled,
    #[error("skill not found: {0}")]
    SkillNotFound(String),
    #[error("agent does not support skills: {0:?}")]
    UnsupportedAgent(AgentType),
    #[error("a real directory already exists at '{path}'")]
    NameCollision { path: String },
    #[error("a different link already exists at '{path}' (points to '{found}')")]
    ForeignLink { path: String, found: String },
    #[error("io error: {0}")]
    Io(String),
    #[error("command failed: {0}")]
    CommandFailed(String),
}

impl Serialize for OfficeToolsError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<io::Error> for OfficeToolsError {
    fn from(err: io::Error) -> Self {
        OfficeToolsError::Io(err.to_string())
    }
}

// ─── Concurrency ───────────────────────────────────────────────────────

fn mutation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

// ─── Public types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficecliInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub compatible: bool,
    pub update_required: bool,
    /// 内置引擎始终可用，此字段只报告私有命令入口创建失败。
    pub runtime_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficecliSkill {
    pub id: String,
    pub category: String,
    pub icon: String,
    pub sort_order: i32,
    pub display_name: BTreeMap<String, String>,
    pub description: BTreeMap<String, String>,
    pub installed_centrally: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSyncReport {
    pub synced: usize,
    pub errors: Vec<String>,
}

const OFFICE_FONT_POLICY_MARKER: &str = "## iyw-claw managed office typography";
const OFFICE_FONT_POLICY_END: &str = "<!-- iyw-claw managed office typography end -->";
const OFFICE_FONT_POLICY: &str = include_str!("../../resources/office-typography.md");

fn office_font_policy(skill_id: &str) -> Option<&'static str> {
    match skill_id {
        "officecli-pptx"
        | "officecli-pitch-deck"
        | "morph-ppt"
        | "morph-ppt-3d"
        | "officecli-docx"
        | "officecli-academic-paper"
        | "officecli-xlsx"
        | "officecli-financial-model"
        | "officecli-data-dashboard" => Some(OFFICE_FONT_POLICY),
        _ => None,
    }
}

fn apply_office_font_policy(skill_id: &str, content: &str) -> String {
    let Some(policy) = office_font_policy(skill_id) else {
        return content.to_owned();
    };
    // 旧规则追加在末尾；新规则用结束标记保留后面的原始技能正文。
    let content = match content.split_once(OFFICE_FONT_POLICY_MARKER) {
        Some((before, managed)) => {
            let after = managed
                .split_once(OFFICE_FONT_POLICY_END)
                .map(|(_, after)| after)
                .unwrap_or_default();
            format!("{before}{after}")
        }
        None => content.to_owned(),
    };
    let mut frontmatter_end = 0;
    if content.lines().next() == Some("---") {
        let mut offset = 0;
        for (index, line) in content.split_inclusive('\n').enumerate() {
            offset += line.len();
            if index > 0 && line.trim_end() == "---" {
                frontmatter_end = offset;
                break;
            }
        }
    }
    let (frontmatter, body) = content.split_at(frontmatter_end);
    format!("{frontmatter}\n{}\n\n{}", policy.trim(), body.trim_start())
}

// ─── Skill metadata (hardcoded — OfficeCLI has no list command) ────────

struct SkillDef {
    /// Canonical skill identity used throughout iyw-claw: the central-store
    /// directory name (`~/.iyw-claw/skills/<id>/`) and the agent invocation
    /// name (`/<id>`). Matches the SKILL.md frontmatter `name:` so the
    /// directory and the skill's self-declared name agree.
    id: &'static str,
    /// Argument passed to `officecli load_skill <load_id>`. The CLI uses
    /// short ids (`pptx`, `word`, `excel`) that differ from the invocation
    /// name (`officecli-pptx`, `officecli-docx`, `officecli-xlsx`); for the
    /// morph skills the two happen to coincide.
    load_id: &'static str,
    category: &'static str,
    icon: &'static str,
    sort_order: i32,
    en_name: &'static str,
    en_desc: &'static str,
    zh_name: &'static str,
    zh_desc: &'static str,
}

const SKILLS: &[SkillDef] = &[
    SkillDef {
        id: "officecli-pptx",
        load_id: "pptx",
        category: "presentations",
        icon: "Presentation",
        sort_order: 10,
        en_name: "Presentation",
        en_desc: "Generic presentations — board reviews, sales decks, all-hands",
        zh_name: "演示文稿",
        zh_desc: "通用演示文稿——评审、销售汇报、全员大会",
    },
    SkillDef {
        id: "officecli-pitch-deck",
        load_id: "pitch-deck",
        category: "presentations",
        icon: "Rocket",
        sort_order: 20,
        en_name: "Pitch Deck",
        en_desc: "Fundraising pitch decks — Seed, Series A–C, SAFE, convertible",
        zh_name: "融资路演",
        zh_desc: "融资路演 PPT——种子轮、A–C 轮、SAFE、可转换票据",
    },
    SkillDef {
        id: "morph-ppt",
        load_id: "morph-ppt",
        category: "presentations",
        icon: "Clapperboard",
        sort_order: 30,
        en_name: "Morph Animation PPT",
        en_desc: "Cinematic Morph-animated presentations",
        zh_name: "Morph 动画 PPT",
        zh_desc: "电影级 Morph 过渡动画演示文稿",
    },
    SkillDef {
        id: "morph-ppt-3d",
        load_id: "morph-ppt-3d",
        category: "presentations",
        icon: "Box",
        sort_order: 40,
        en_name: "3D Morph PPT",
        en_desc: "3D Morph with GLB models and camera moves",
        zh_name: "3D Morph PPT",
        zh_desc: "3D Morph + GLB 模型 + 镜头运动",
    },
    SkillDef {
        id: "officecli-docx",
        load_id: "word",
        category: "documents",
        icon: "FileText",
        sort_order: 50,
        en_name: "Word Document",
        en_desc: "Reports, letters, memos, proposals",
        zh_name: "Word 文档",
        zh_desc: "报告、信件、备忘录、提案",
    },
    SkillDef {
        id: "officecli-academic-paper",
        load_id: "academic-paper",
        category: "documents",
        icon: "GraduationCap",
        sort_order: 60,
        en_name: "Academic Paper",
        en_desc: "Journal/thesis with citations, equations, cross-refs",
        zh_name: "学术论文",
        zh_desc: "期刊论文——引用、公式、交叉引用",
    },
    SkillDef {
        id: "officecli-xlsx",
        load_id: "excel",
        category: "spreadsheets",
        icon: "FileSpreadsheet",
        sort_order: 70,
        en_name: "Excel Workbook",
        en_desc: "Generic workbooks, formulas, pivots, trackers",
        zh_name: "Excel 工作簿",
        zh_desc: "通用工作簿——公式、数据透视表、追踪表",
    },
    SkillDef {
        id: "officecli-financial-model",
        load_id: "financial-model",
        category: "spreadsheets",
        icon: "TrendingUp",
        sort_order: 80,
        en_name: "Financial Model",
        en_desc: "3-statement, DCF, LBO, scenarios, projections",
        zh_name: "财务模型",
        zh_desc: "三大报表、DCF、LBO、情景分析、预测",
    },
    SkillDef {
        id: "officecli-data-dashboard",
        load_id: "data-dashboard",
        category: "spreadsheets",
        icon: "BarChart3",
        sort_order: 90,
        en_name: "Data Dashboard",
        en_desc: "CSV/tabular data → KPI/analytics Excel dashboards",
        zh_name: "数据仪表盘",
        zh_desc: "CSV/表格数据 → KPI/分析 Excel 仪表盘",
    },
];

fn skill_defs() -> &'static [SkillDef] {
    SKILLS
}

fn find_skill_def(id: &str) -> Option<&'static SkillDef> {
    skill_defs().iter().find(|s| s.id == id)
}

const OFFICECLI_BOOTSTRAP_MARKER: &str = ".officecli-embedded-bootstrap-complete.v1";
const OFFICECLI_BOOTSTRAP_DISABLED_MARKER: &str = ".officecli-bootstrap-disabled.v1";
const OFFICECLI_MINIMUM_VERSION: &str = crate::office_embedded::VERSION;

type OfficecliBootstrapResult = Result<SkillSyncReport, OfficeToolsError>;

struct OfficecliBootstrapAttempt {
    result: watch::Sender<Option<OfficecliBootstrapResult>>,
}

impl OfficecliBootstrapAttempt {
    fn new() -> Self {
        let (result, _) = watch::channel(None);
        Self { result }
    }

    async fn wait(&self) -> OfficecliBootstrapResult {
        let mut receiver = self.result.subscribe();
        receiver
            .wait_for(|result| result.is_some())
            .await
            .expect("bootstrap attempt sender remains alive");
        let result = receiver
            .borrow()
            .clone()
            .expect("completed bootstrap publishes a result");
        result
    }

    fn complete(&self, result: OfficecliBootstrapResult) {
        self.result.send_replace(Some(result));
    }
}

fn bootstrap_attempts() -> &'static StdMutex<HashMap<PathBuf, Arc<OfficecliBootstrapAttempt>>> {
    static ATTEMPTS: OnceLock<StdMutex<HashMap<PathBuf, Arc<OfficecliBootstrapAttempt>>>> =
        OnceLock::new();
    ATTEMPTS.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn acquire_bootstrap_attempt(marker_path: &Path) -> (Arc<OfficecliBootstrapAttempt>, bool) {
    let mut attempts = bootstrap_attempts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(attempt) = attempts.get(marker_path) {
        return (Arc::clone(attempt), false);
    }
    let attempt = Arc::new(OfficecliBootstrapAttempt::new());
    attempts.insert(marker_path.to_path_buf(), Arc::clone(&attempt));
    (attempt, true)
}

struct BootstrapAttemptOwner {
    marker_path: PathBuf,
    attempt: Arc<OfficecliBootstrapAttempt>,
}

impl Drop for BootstrapAttemptOwner {
    fn drop(&mut self) {
        if self.attempt.result.borrow().is_none() {
            self.attempt.complete(Err(OfficeToolsError::CommandFailed(
                "OfficeCLI bootstrap attempt was cancelled".into(),
            )));
        }
        let mut attempts = bootstrap_attempts()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let owned = attempts
            .get(&self.marker_path)
            .is_some_and(|current| Arc::ptr_eq(current, &self.attempt));
        if owned {
            attempts.remove(&self.marker_path);
        }
    }
}

fn officecli_bootstrap_marker_path(data_dir: &Path) -> PathBuf {
    data_dir.join(OFFICECLI_BOOTSTRAP_MARKER)
}

fn officecli_bootstrap_disabled_marker_path(data_dir: &Path) -> PathBuf {
    data_dir.join(OFFICECLI_BOOTSTRAP_DISABLED_MARKER)
}

fn officecli_bootstrap_assets_complete(info: &OfficecliInfo, skill_root: &Path) -> bool {
    info.installed
        && info.runtime_error.is_none()
        && info.compatible
        && skill_defs()
            .iter()
            .all(|skill| office_skill_source_is_ready(&skill_root.join(skill.id)))
}

fn office_skill_source_is_ready(source: &Path) -> bool {
    let skill_path = source.join("SKILL.md");
    skill_path.is_file()
        && (crate::commands::acp::read_market_skill_marker(source).is_some()
            || office_skill_policy_is_current(&skill_path))
}

fn office_skill_policy_is_current(skill_path: &Path) -> bool {
    fs::read_to_string(skill_path)
        .map(|content| {
            let content = content.replace("\r\n", "\n");
            let policy = OFFICE_FONT_POLICY.replace("\r\n", "\n");
            content.contains(policy.trim())
        })
        .unwrap_or(false)
}

fn mark_officecli_bootstrap_disabled(data_dir: &Path) -> Result<(), OfficeToolsError> {
    fs::create_dir_all(data_dir)?;
    let complete = officecli_bootstrap_marker_path(data_dir);
    if complete.exists() {
        fs::remove_file(complete)?;
    }
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(officecli_bootstrap_disabled_marker_path(data_dir))?;
    Ok(())
}

fn clear_officecli_bootstrap_disabled(data_dir: &Path) -> Result<(), OfficeToolsError> {
    let disabled = officecli_bootstrap_disabled_marker_path(data_dir);
    if disabled.exists() {
        fs::remove_file(disabled)?;
    }
    Ok(())
}

#[cfg(feature = "tauri-runtime")]
fn tauri_effective_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, OfficeToolsError> {
    use tauri::Manager;

    let fallback = app
        .path()
        .app_data_dir()
        .map_err(|error| OfficeToolsError::Io(error.to_string()))?;
    Ok(crate::paths::resolve_effective_data_dir(&fallback))
}

async fn run_officecli_bootstrap_once<V, VFut, F, Fut>(
    marker_path: &Path,
    disabled_marker_path: &Path,
    expected_sync_count: usize,
    completion_is_valid: V,
    operation: F,
) -> Result<SkillSyncReport, OfficeToolsError>
where
    V: FnOnce() -> VFut,
    VFut: Future<Output = Result<bool, OfficeToolsError>>,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<SkillSyncReport, OfficeToolsError>>,
{
    let (attempt, is_owner) = acquire_bootstrap_attempt(marker_path);
    if !is_owner {
        return attempt.wait().await;
    }
    let _owner = BootstrapAttemptOwner {
        marker_path: marker_path.to_path_buf(),
        attempt: Arc::clone(&attempt),
    };

    let _guard = mutation_lock().lock().await;
    let result = async {
        if disabled_marker_path.is_file() {
            return Ok(SkillSyncReport {
                synced: 0,
                errors: vec![],
            });
        }
        if disabled_marker_path.exists() {
            return Err(OfficeToolsError::Io(format!(
                "OfficeCLI bootstrap opt-out marker is not a file: {}",
                disabled_marker_path.display()
            )));
        }
        if marker_path.is_file() && completion_is_valid().await? {
            return Ok(SkillSyncReport {
                synced: 0,
                errors: vec![],
            });
        }
        if marker_path.exists() {
            fs::remove_file(marker_path)?;
        }

        let report = operation().await?;
        if report.errors.is_empty() && report.synced == expected_sync_count {
            if let Some(parent) = marker_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(marker_path)?;
        }
        Ok(report)
    }
    .await;
    attempt.complete(result.clone());
    result
}

pub(crate) fn is_officecli_skill_id(id: &str) -> bool {
    find_skill_def(id).is_some()
}

pub(crate) fn managed_office_dependency_available(id: &str) -> bool {
    is_officecli_skill_id(id)
        && resolve_officecli().is_some()
        && office_skill_source_is_ready(&skill_central_path(id))
}

fn skill_def_to_metadata(def: &SkillDef) -> OfficecliSkill {
    let mut display_name = BTreeMap::new();
    display_name.insert("en".to_string(), def.en_name.to_string());
    display_name.insert("zh-CN".to_string(), def.zh_name.to_string());

    let mut description = BTreeMap::new();
    description.insert("en".to_string(), def.en_desc.to_string());
    description.insert("zh-CN".to_string(), def.zh_desc.to_string());

    let central_path = skill_central_path(def.id);
    OfficecliSkill {
        id: def.id.to_string(),
        category: def.category.to_string(),
        icon: def.icon.to_string(),
        sort_order: def.sort_order,
        display_name,
        description,
        installed_centrally: central_path.exists(),
    }
}

// ─── Path helpers ──────────────────────────────────────────────────────

fn skill_central_path(skill_id: &str) -> PathBuf {
    central_experts_dir().join(skill_id)
}

fn agent_link_path(agent: AgentType, skill_id: &str) -> Result<PathBuf, OfficeToolsError> {
    let dir = preferred_scope_skill_dir(agent, AgentSkillScope::Global, None)
        .map_err(|_| OfficeToolsError::UnsupportedAgent(agent))?;
    Ok(dir.join(skill_id))
}

// ─── 内置办公引擎 ─────────────────────────────────────────────────────

pub(crate) fn resolve_officecli() -> Option<PathBuf> {
    crate::office_embedded::agent_entrypoint()
        .map_err(|error| {
            tracing::error!(%error, "[office] embedded command entry unavailable");
        })
        .ok()
}

pub(crate) fn officecli_agent_path_dir() -> Option<PathBuf> {
    resolve_officecli().and_then(|path| path.parent().map(Path::to_path_buf))
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn officecli_detect() -> OfficecliInfo {
    let result = crate::office_embedded::agent_entrypoint();
    OfficecliInfo {
        installed: true,
        compatible: result.is_ok(),
        update_required: false,
        version: Some(crate::office_embedded::VERSION.into()),
        path: result
            .as_ref()
            .ok()
            .map(|path| path.to_string_lossy().into_owned()),
        runtime_error: result.err(),
    }
}

// ─── Commands: install / uninstall ─────────────────────────────────────

// ─── Streamed install progress events ──────────────────────────────────
//
// The official installer downloads a multi-MB binary; on a slow network that
// looks like a hang. Mirror the ACP agent-install UX: stream the installer's
// stdout/stderr to the settings page line-by-line over a dedicated event
// channel, tagged with the caller's `task_id` so concurrent installs don't
// cross-contaminate. Shape matches `AgentInstallEvent` in `commands::acp`.

const OFFICECLI_INSTALL_EVENT: &str = "app://officecli-install";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum OfficecliInstallEventKind {
    Started,
    Log,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
struct OfficecliInstallEvent {
    task_id: String,
    kind: OfficecliInstallEventKind,
    payload: String,
}

fn emit_officecli_install_event(
    emitter: &EventEmitter,
    task_id: &str,
    kind: OfficecliInstallEventKind,
    payload: impl Into<String>,
) {
    crate::web::event_bridge::emit_event(
        emitter,
        OFFICECLI_INSTALL_EVENT,
        OfficecliInstallEvent {
            task_id: task_id.to_string(),
            kind,
            payload: payload.into(),
        },
    );
}

/// Tauri command: run the OfficeCLI installer, streaming progress as
/// `app://officecli-install` events tagged with `task_id`. The work lives in
/// `officecli_install_core` so the web handler can share it with its own emitter.
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn officecli_install(
    task_id: String,
    app: tauri::AppHandle,
) -> Result<OfficecliInfo, OfficeToolsError> {
    let data_dir = tauri_effective_data_dir(&app)?;
    officecli_install_core(&data_dir, task_id, &EventEmitter::Tauri(app)).await
}

/// 桌面版通过统一环境修复安装，服务器版使用官方安装脚本；均转发安装进度。
pub(crate) async fn officecli_install_core(
    data_dir: &Path,
    task_id: String,
    emitter: &EventEmitter,
) -> Result<OfficecliInfo, OfficeToolsError> {
    emit_officecli_install_event(emitter, &task_id, OfficecliInstallEventKind::Started, "");

    // Install and uninstall share `mutation_lock`. Acquire it AFTER the first
    // stream event so the panel is responsive immediately, and surface a waiting
    // hint when another operation already holds it (e.g. a second web client)
    // rather than spinning silently for up to the install timeout.
    let _guard = match mutation_lock().try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            emit_officecli_install_event(
                emitter,
                &task_id,
                OfficecliInstallEventKind::Log,
                "Waiting for another OfficeCLI operation to finish…",
            );
            mutation_lock().lock().await
        }
    };

    // Another client may have completed installation while this request waited
    // for the process-wide mutation lock. Re-check under the lock so concurrent
    // startup/manual requests never run the vendor installer twice.
    let existing = officecli_detect().await;
    if existing.installed && existing.runtime_error.is_none() && existing.compatible {
        emit_officecli_install_event(
            emitter,
            &task_id,
            OfficecliInstallEventKind::Completed,
            "OfficeCLI is already installed",
        );
        clear_officecli_bootstrap_disabled(data_dir)?;
        return Ok(existing);
    }

    let info = officecli_install_locked(task_id, emitter).await?;
    clear_officecli_bootstrap_disabled(data_dir)?;
    Ok(info)
}

/// Run the installer while the caller holds `mutation_lock`.
async fn officecli_install_locked(
    task_id: String,
    emitter: &EventEmitter,
) -> Result<OfficecliInfo, OfficeToolsError> {
    let info = officecli_detect().await;
    if let Some(error) = &info.runtime_error {
        return Err(OfficeToolsError::CommandFailed(error.clone()));
    }
    emit_officecli_install_event(
        emitter,
        &task_id,
        OfficecliInstallEventKind::Completed,
        "Built-in office engine is ready",
    );
    Ok(info)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn officecli_uninstall(app: tauri::AppHandle) -> Result<OfficecliInfo, OfficeToolsError> {
    let data_dir = tauri_effective_data_dir(&app)?;
    officecli_uninstall_core(&data_dir).await
}

pub(crate) async fn officecli_uninstall_core(
    data_dir: &Path,
) -> Result<OfficecliInfo, OfficeToolsError> {
    let _guard = mutation_lock().lock().await;
    mark_officecli_bootstrap_disabled(data_dir)?;

    let mut cleanup_errors: Vec<String> = Vec::new();

    // Remove per-agent symlinks across all scoped skill dirs (not just the
    // preferred dir) so secondary dirs like ~/.agents/skills are also cleaned.
    for agent in supported_agents() {
        let dirs = match scoped_skill_dirs(agent, AgentSkillScope::Global, None) {
            Ok(d) => d,
            Err(_) => continue,
        };
        for def in skill_defs() {
            let _shared_guard = crate::commands::acp::shared_skill_mutation_guard();
            let central = skill_central_path(def.id);
            if crate::commands::acp::read_market_skill_marker(&central).is_some() {
                continue;
            }
            for dir in &dirs {
                let candidate = dir.join(def.id);
                if !candidate.exists() && !path_is_symlink(&candidate) {
                    continue;
                }
                let state = classify_link(&candidate, &central);
                let should_remove = match state {
                    ExpertLinkState::LinkedToIywClaw => true,
                    ExpertLinkState::Broken => {
                        // Only remove broken links whose target was our
                        // central skill dir (not user-owned danglers).
                        read_link_target(&candidate)
                            .map(|t| t.starts_with(&central))
                            .unwrap_or(false)
                    }
                    _ => false,
                };
                if should_remove {
                    if let Err(e) = remove_skill_entry(&candidate) {
                        cleanup_errors.push(format!(
                            "failed to remove link {}: {e}",
                            candidate.display()
                        ));
                    }
                }
            }
        }
    }

    // Clean up OfficeCLI skills from central store
    for def in skill_defs() {
        let _shared_guard = crate::commands::acp::shared_skill_mutation_guard();
        let central = skill_central_path(def.id);
        if crate::commands::acp::read_market_skill_marker(&central).is_some() {
            continue;
        }
        if central.exists() {
            if let Err(e) = fs::remove_dir_all(&central) {
                cleanup_errors.push(format!("failed to remove {}: {e}", central.display()));
            }
        }
    }

    if !cleanup_errors.is_empty() {
        return Err(OfficeToolsError::Io(format!(
            "office skills disabled but cleanup had errors: {}",
            cleanup_errors.join("; ")
        )));
    }

    // Re-detect so the caller sees the real post-uninstall state (e.g. a
    // system/Homebrew binary may still be on PATH).
    Ok(officecli_detect().await)
}

// ─── Commands: skill listing ───────────────────────────────────────────

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn officecli_list_skills() -> Vec<OfficecliSkill> {
    skill_defs().iter().map(skill_def_to_metadata).collect()
}

// ─── Commands: skill sync ──────────────────────────────────────────────

pub(crate) async fn officecli_sync_skills_core() -> Result<SkillSyncReport, OfficeToolsError> {
    let _guard = mutation_lock().lock().await;
    officecli_sync_skills_locked().await
}

/// 为市场包准备可用的 OfficeCLI 技能依赖；直接安装市场版本走市场覆盖流程。
/// 已有市场版本由市场维护，其他 OfficeCLI 技能由 Office Tools 同步。
pub(crate) async fn ensure_managed_office_skill_ready(
    skill_id: &str,
) -> Result<(), OfficeToolsError> {
    if find_skill_def(skill_id).is_none() {
        return Err(OfficeToolsError::SkillNotFound(skill_id.to_string()));
    }
    let info = officecli_detect().await;
    if !info.installed {
        return Err(OfficeToolsError::NotInstalled);
    }
    if let Some(runtime_error) = info.runtime_error {
        return Err(OfficeToolsError::CommandFailed(runtime_error));
    }
    let mut needs_sync = false;
    if !info.compatible {
        let current = info.version.as_deref().unwrap_or("unknown");
        let data_dir = std::env::var_os("IYW_CLAW_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| dirs::data_dir().map(|path| path.join("iyw-claw")))
            .unwrap_or_else(|| PathBuf::from(".iyw-claw-data"));
        officecli_install_core(
            &data_dir,
            "skill-market-officecli-update".to_string(),
            &EventEmitter::Noop,
        )
        .await
        .map_err(|error| {
            OfficeToolsError::CommandFailed(format!(
                "OfficeCLI {current} requires update to {OFFICECLI_MINIMUM_VERSION}: {error}"
            ))
        })?;
        needs_sync = true;
    }
    let source = skill_central_path(skill_id);
    if needs_sync || !office_skill_source_is_ready(&source) {
        officecli_sync_skills_core().await?;
    }
    if office_skill_source_is_ready(&source) {
        Ok(())
    } else {
        Err(OfficeToolsError::SkillNotFound(skill_id.to_string()))
    }
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn officecli_sync_skills(
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<SkillSyncReport, OfficeToolsError> {
    let report = officecli_sync_skills_core().await?;
    if let Err(error) = crate::commands::managed_skills::reconcile_persisted_family_core(
        &db.conn,
        crate::commands::managed_skills::ManagedSkillFamily::OfficeTools,
    )
    .await
    {
        tracing::warn!("[office] managed skill reconcile after sync failed: {error}");
    }
    Ok(report)
}

/// Sync the bundled OfficeCLI skills while the caller holds `mutation_lock`.
async fn officecli_sync_skills_locked() -> Result<SkillSyncReport, OfficeToolsError> {
    fs::create_dir_all(central_experts_dir())?;
    let mut report = SkillSyncReport {
        synced: 0,
        errors: vec![],
    };
    for def in skill_defs() {
        let target = skill_central_path(def.id);
        if crate::commands::acp::read_market_skill_marker(&target).is_some() {
            if target.join("SKILL.md").is_file() {
                report.synced += 1;
            } else {
                report.errors.push(format!(
                    "{}: market Skill content is missing; reinstall it from Skill Market",
                    def.id
                ));
            }
            continue;
        }
        match crate::office_embedded::skill(def.load_id) {
            Ok(content) if !content.trim().is_empty() => {
                write_synced_office_skill(def, &content)?;
                report.synced += 1;
            }
            Ok(_) => report
                .errors
                .push(format!("{}: empty bundled skill", def.id)),
            Err(error) => report.errors.push(format!("{}: {error}", def.id)),
        }
    }
    tracing::info!(
        synced = report.synced,
        errors = report.errors.len(),
        "[office] bundled skills synchronized"
    );
    Ok(report)
}

fn write_synced_office_skill(def: &SkillDef, content: &str) -> Result<(), OfficeToolsError> {
    let _guard = crate::commands::acp::shared_skill_mutation_guard();
    let target_dir = skill_central_path(def.id);
    if crate::commands::acp::read_market_skill_marker(&target_dir).is_some() {
        tracing::debug!(
            skill_id = def.id,
            "[office] market Skill installed during sync; keeping override"
        );
        return Ok(());
    }
    fs::create_dir_all(&target_dir)?;
    fs::write(
        target_dir.join("SKILL.md"),
        apply_office_font_policy(def.id, content),
    )?;
    Ok(())
}

/// Atomically perform the backend's one-time OfficeCLI install + skill sync.
/// A partial or failed sync leaves the marker absent so a later startup retries.
pub(crate) async fn officecli_bootstrap_core(
    data_dir: &Path,
    task_id: String,
    emitter: &EventEmitter,
) -> Result<SkillSyncReport, OfficeToolsError> {
    let marker_path = officecli_bootstrap_marker_path(data_dir);
    let disabled_marker_path = officecli_bootstrap_disabled_marker_path(data_dir);
    run_officecli_bootstrap_once(
        &marker_path,
        &disabled_marker_path,
        skill_defs().len(),
        || async {
            let info = officecli_detect().await;
            Ok(officecli_bootstrap_assets_complete(
                &info,
                &central_experts_dir(),
            ))
        },
        || async move {
            let info = officecli_detect().await;
            if !info.installed || info.runtime_error.is_some() || !info.compatible {
                emit_officecli_install_event(
                    emitter,
                    &task_id,
                    OfficecliInstallEventKind::Started,
                    "",
                );
                officecli_install_locked(task_id, emitter).await?;
            }
            officecli_sync_skills_locked().await
        },
    )
    .await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn officecli_bootstrap(
    task_id: String,
    app: tauri::AppHandle,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<SkillSyncReport, OfficeToolsError> {
    let data_dir = tauri_effective_data_dir(&app)?;
    let report = officecli_bootstrap_core(&data_dir, task_id, &EventEmitter::Tauri(app)).await?;
    if let Err(error) = crate::commands::managed_skills::reconcile_persisted_family_core(
        &db.conn,
        crate::commands::managed_skills::ManagedSkillFamily::OfficeTools,
    )
    .await
    {
        tracing::warn!("[office] managed skill reconcile after bootstrap failed: {error}");
    }
    Ok(report)
}

// ─── Commands: skill link / unlink ─────────────────────────────────────

fn supported_agents() -> Vec<AgentType> {
    crate::commands::managed_skills::supported_skill_agent_types()
}

pub(crate) fn managed_office_skill_ids() -> Vec<String> {
    skill_defs()
        .iter()
        .map(|definition| definition.id.to_string())
        .collect()
}

pub(crate) fn managed_ready_office_skill_ids() -> Vec<String> {
    skill_defs()
        .iter()
        .filter(|definition| skill_central_path(definition.id).exists())
        .map(|definition| definition.id.to_string())
        .collect()
}

pub(crate) fn managed_office_skill_has_owned_link(skill_id: &str, agents: &[AgentType]) -> bool {
    let expected = skill_central_path(skill_id);
    agents.iter().any(|agent_type| {
        scoped_skill_dirs(*agent_type, AgentSkillScope::Global, None).is_ok_and(|dirs| {
            dirs.into_iter()
                .any(|dir| managed_link_is_owned(&expected, &dir.join(skill_id)))
        })
    })
}

fn office_error_from_managed(error: ManagedLinkEntryError, link_path: &Path) -> OfficeToolsError {
    let path = link_path.to_string_lossy().to_string();
    match error {
        ManagedLinkEntryError::NameCollision => OfficeToolsError::NameCollision { path },
        ManagedLinkEntryError::ForeignLink { found } => {
            OfficeToolsError::ForeignLink { path, found }
        }
        ManagedLinkEntryError::Io(message) => OfficeToolsError::Io(message),
    }
}

/// Link one office skill into one agent's skill dir. **Assumes the mutation
/// lock is already held** — `tokio::sync::Mutex` is not reentrant, so
/// `officecli_skill_apply_links` locks once and calls this directly rather than
/// the public command. The "not synced" guard stays here so a batch enable of
/// an un-synced skill fails only that op.
fn link_one_locked(
    skill_id: &str,
    agent_type: AgentType,
) -> Result<ExpertInstallStatus, OfficeToolsError> {
    let skill_id = validate_skill_id(skill_id).map_err(|e| OfficeToolsError::Io(e.to_string()))?;
    let _ = find_skill_def(&skill_id)
        .ok_or_else(|| OfficeToolsError::SkillNotFound(skill_id.clone()))?;

    let central = skill_central_path(&skill_id);
    if !central.exists() {
        return Err(OfficeToolsError::Io(format!(
            "skill '{skill_id}' is not synced — run sync first"
        )));
    }

    let link_path = agent_link_path(agent_type, &skill_id)?;
    let change = reconcile_managed_link_entry(&central, &link_path, true)
        .map_err(|error| office_error_from_managed(error, &link_path))?;
    let copy_mode = matches!(change, ManagedLinkChange::Linked { copy_mode: true });

    let state = classify_link(&link_path, &central);
    let target_path = read_link_target(&link_path).map(|p| p.to_string_lossy().to_string());
    Ok(ExpertInstallStatus {
        expert_id: skill_id.clone(),
        agent_type,
        state,
        link_path: link_path.to_string_lossy().to_string(),
        target_path,
        expected_target_path: central.to_string_lossy().to_string(),
        copy_mode,
    })
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn officecli_skill_link_to_agent(
    skill_id: String,
    agent_type: AgentType,
) -> Result<ExpertInstallStatus, OfficeToolsError> {
    let _guard = mutation_lock().lock().await;
    link_one_locked(&skill_id, agent_type)
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn officecli_skill_unlink_from_agent(
    skill_id: String,
    agent_type: AgentType,
) -> Result<(), OfficeToolsError> {
    let _guard = mutation_lock().lock().await;
    unlink_one_locked(&skill_id, agent_type)
}

/// Remove one office skill's link from one agent's skill dirs. **Assumes the
/// mutation lock is already held** (see `link_one_locked`).
fn unlink_one_locked(skill_id: &str, agent_type: AgentType) -> Result<(), OfficeToolsError> {
    let skill_id = validate_skill_id(skill_id).map_err(|e| OfficeToolsError::Io(e.to_string()))?;
    let _ = find_skill_def(&skill_id)
        .ok_or_else(|| OfficeToolsError::SkillNotFound(skill_id.clone()))?;

    let dirs = scoped_skill_dirs(agent_type, AgentSkillScope::Global, None)
        .map_err(|_| OfficeToolsError::UnsupportedAgent(agent_type))?;

    let central = skill_central_path(&skill_id);
    for dir in dirs {
        let candidate = dir.join(&skill_id);
        if !candidate.exists() && !path_is_symlink(&candidate) {
            continue;
        }
        let state = classify_link(&candidate, &central);
        if managed_link_is_owned(&central, &candidate) {
            remove_skill_entry(&candidate).map_err(|e| {
                OfficeToolsError::Io(format!("remove link {}: {e}", candidate.display()))
            })?;
        } else if state == ExpertLinkState::LinkedElsewhere {
            return Err(OfficeToolsError::ForeignLink {
                path: candidate.to_string_lossy().to_string(),
                found: read_link_target(&candidate)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "<unknown>".into()),
            });
        }
    }
    Ok(())
}

fn office_status_from_link_change(
    skill_id: &str,
    agent_type: AgentType,
    link_change: (PathBuf, bool),
) -> ExpertInstallStatus {
    let (link_path, copy_mode) = link_change;
    let central = skill_central_path(skill_id);
    ExpertInstallStatus {
        expert_id: skill_id.to_string(),
        agent_type,
        state: classify_link(&link_path, &central),
        link_path: link_path.to_string_lossy().to_string(),
        target_path: read_link_target(&link_path).map(|path| path.to_string_lossy().to_string()),
        expected_target_path: central.to_string_lossy().to_string(),
        copy_mode,
    }
}

fn managed_office_link_paths(
    skill_id: &str,
    agent_type: AgentType,
) -> Result<(PathBuf, Vec<PathBuf>), OfficeToolsError> {
    let preferred = agent_link_path(agent_type, skill_id)?;
    let paths = scoped_skill_dirs(agent_type, AgentSkillScope::Global, None)
        .map_err(|_| OfficeToolsError::UnsupportedAgent(agent_type))?
        .into_iter()
        .map(|directory| directory.join(skill_id))
        .collect();
    Ok((preferred, paths))
}

fn managed_office_pair_result(
    skill_id: &str,
    agent_type: AgentType,
    enable: bool,
) -> Option<LinkOpResult> {
    let _shared_guard = crate::commands::acp::shared_skill_mutation_guard();
    let central = skill_central_path(skill_id);
    if crate::commands::acp::read_market_skill_marker(&central).is_some() {
        return None;
    }
    if enable && !central.exists() {
        return None;
    }
    let (preferred, paths) = match managed_office_link_paths(skill_id, agent_type) {
        Ok(paths) => paths,
        Err(error) => return Some(office_link_failure(skill_id, agent_type, error.to_string())),
    };
    match reconcile_managed_link_paths(&central, &preferred, &paths, enable) {
        Ok(changes) if changes.is_empty() => None,
        Ok(_) if !enable => Some(office_link_success(skill_id, agent_type, None)),
        Ok(changes) => changes.into_iter().find_map(|(path, change)| {
            let ManagedLinkChange::Linked { copy_mode } = change else {
                return None;
            };
            let status = office_status_from_link_change(skill_id, agent_type, (path, copy_mode));
            Some(office_link_success(skill_id, agent_type, Some(status)))
        }),
        Err((path, error)) => Some(office_link_failure(
            skill_id,
            agent_type,
            office_error_from_managed(error, &path).to_string(),
        )),
    }
}

fn office_link_success(
    skill_id: &str,
    agent_type: AgentType,
    status: Option<ExpertInstallStatus>,
) -> LinkOpResult {
    LinkOpResult {
        expert_id: skill_id.to_string(),
        agent_type,
        ok: true,
        status,
        error: None,
    }
}

fn office_link_failure(skill_id: &str, agent_type: AgentType, error: String) -> LinkOpResult {
    LinkOpResult {
        expert_id: skill_id.to_string(),
        agent_type,
        ok: false,
        status: None,
        error: Some(error),
    }
}

pub(crate) async fn reconcile_managed_office_tools(
    targets: &[(AgentType, String, bool)],
) -> Vec<LinkOpResult> {
    let guard = mutation_lock().lock().await;
    let operations = targets.to_vec();
    tokio::task::spawn_blocking(move || {
        let _guard = guard;
        operations
            .iter()
            .filter_map(|(agent_type, skill_id, enable)| {
                managed_office_pair_result(skill_id, *agent_type, *enable)
            })
            .collect()
    })
    .await
    .unwrap_or_else(|error| {
        tracing::error!(%error, "[skills] office skill reconciliation task failed");
        targets
            .iter()
            .map(|(agent, id, _)| office_link_failure(id, *agent, error.to_string()))
            .collect()
    })
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn officecli_skill_get_install_status(
    skill_id: String,
) -> Result<Vec<ExpertInstallStatus>, OfficeToolsError> {
    let skill_id = validate_skill_id(&skill_id).map_err(|e| OfficeToolsError::Io(e.to_string()))?;
    let _ = find_skill_def(&skill_id)
        .ok_or_else(|| OfficeToolsError::SkillNotFound(skill_id.clone()))?;

    let expected = skill_central_path(&skill_id);
    let agents = supported_agents();

    let mut out = Vec::with_capacity(agents.len());
    for agent in agents {
        let link_path = match agent_link_path(agent, &skill_id) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let state = classify_link(&link_path, &expected);
        let target_path = read_link_target(&link_path).map(|p| p.to_string_lossy().to_string());
        out.push(ExpertInstallStatus {
            expert_id: skill_id.clone(),
            agent_type: agent,
            state,
            link_path: link_path.to_string_lossy().to_string(),
            target_path,
            expected_target_path: expected.to_string_lossy().to_string(),
            copy_mode: managed_copy_is_owned(&expected, &link_path),
        });
    }
    Ok(out)
}

/// Apply a batch of enable/disable operations under a single lock acquisition.
/// Mirrors `experts_apply_links`; an un-synced skill simply fails its own op.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn officecli_skill_apply_links(
    ops: Vec<LinkOp>,
) -> Result<Vec<LinkOpResult>, OfficeToolsError> {
    let _guard = mutation_lock().lock().await;
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        // `LinkOp.expert_id` carries the office skill id here — the field name is
        // shared with the experts batch type, and office's ExpertInstallStatus
        // already overloads `expert_id` as the skill id.
        let LinkOp {
            expert_id,
            agent_type,
            enable,
        } = op;
        let res = if enable {
            link_one_locked(&expert_id, agent_type).map(Some)
        } else {
            unlink_one_locked(&expert_id, agent_type).map(|()| None)
        };
        out.push(match res {
            Ok(status) => LinkOpResult {
                expert_id,
                agent_type,
                ok: true,
                status,
                error: None,
            },
            Err(err) => LinkOpResult {
                expert_id,
                agent_type,
                ok: false,
                status: None,
                error: Some(err.to_string()),
            },
        });
    }
    Ok(out)
}

/// One-shot snapshot of every (skill, agent) link state for the matrix UI.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn officecli_skill_list_all_install_statuses(
) -> Result<Vec<ExpertInstallStatus>, OfficeToolsError> {
    let agents = supported_agents();
    let mut out = Vec::with_capacity(skill_defs().len() * agents.len());
    for def in skill_defs() {
        let expected = skill_central_path(def.id);
        for &agent in &agents {
            let link_path = match agent_link_path(agent, def.id) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let state = classify_link(&link_path, &expected);
            let target_path = read_link_target(&link_path).map(|p| p.to_string_lossy().to_string());
            out.push(ExpertInstallStatus {
                expert_id: def.id.to_string(),
                agent_type: agent,
                state,
                link_path: link_path.to_string_lossy().to_string(),
                target_path,
                expected_target_path: expected.to_string_lossy().to_string(),
                copy_mode: managed_copy_is_owned(&expected, &link_path),
            });
        }
    }
    Ok(out)
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn officecli_skill_read_content(skill_id: String) -> Result<String, OfficeToolsError> {
    let skill_id = validate_skill_id(&skill_id).map_err(|e| OfficeToolsError::Io(e.to_string()))?;
    let _ = find_skill_def(&skill_id)
        .ok_or_else(|| OfficeToolsError::SkillNotFound(skill_id.clone()))?;

    let path = skill_central_path(&skill_id).join("SKILL.md");
    if !path.exists() {
        return Err(OfficeToolsError::Io(format!(
            "skill '{skill_id}' has no SKILL.md — run sync first"
        )));
    }
    let content = fs::read_to_string(&path)?;
    Ok(content)
}

// ─── Commands: office file preview ─────────────────────────────────────

pub(crate) fn is_office_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("docx") | Some("xlsx") | Some("pptx")
    )
}

/// Render an office file (.docx/.xlsx/.pptx) to self-contained HTML via
/// `officecli view <file> html`, for the in-app preview. Runs officecli in
/// iyw-claw's own process (not the agent's command sandbox), so it is unaffected
/// by the sandbox restrictions that can break officecli inside an agent turn.
///
/// `path` is relative to `root_path`; the resolved target is canonicalized and
/// confined to the workspace root, mirroring `read_workspace_file_base64`.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn officecli_render_html(
    root_path: String,
    path: String,
) -> Result<String, OfficeToolsError> {
    let root = PathBuf::from(&root_path);
    if !root.is_dir() {
        return Err(OfficeToolsError::Io(
            "workspace root does not exist".to_string(),
        ));
    }

    let target =
        resolve_tree_path(&root, &path).map_err(|e| OfficeToolsError::Io(e.to_string()))?;

    // Canonicalize + confine within the workspace root (defense in depth: the
    // path comes from an open file tab, but never render outside the root).
    let canonical_root = fs::canonicalize(&root)?;
    let canonical_target = fs::canonicalize(&target)?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(OfficeToolsError::Io(
            "path is outside workspace root".to_string(),
        ));
    }
    if !canonical_target.is_file() {
        return Err(OfficeToolsError::Io("path is not a file".to_string()));
    }
    if !is_office_path(&canonical_target) {
        return Err(OfficeToolsError::Io(
            "not a supported office file (.docx/.xlsx/.pptx)".to_string(),
        ));
    }

    crate::office_watch::render_snapshot(canonical_target)
        .await
        .map_err(|error| OfficeToolsError::CommandFailed(error.to_string()))
}

// ─── Commands: office live preview (watch) ─────────────────────────────

/// Start (or share, by ref-count) a long-lived `officecli watch` HTTP preview
/// server for an office file and return its loopback port. The live preview is
/// driven by officecli's own SSE refresh, so it no longer races the agent's
/// edits for the file on disk (the bug the one-shot `view html` path caused on
/// Windows). See `crate::office_watch`.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn start_office_watch(
    root_path: String,
    path: String,
) -> Result<crate::office_watch::OfficeWatchStarted, AppCommandError> {
    crate::office_watch::start_office_watch_core(
        root_path,
        path,
        crate::office_watch::WatchOrigin::Desktop,
    )
    .await
    .map_err(Into::into)
}

/// Release one reference to the watch preview for an office file; kills the
/// server when the last viewer goes away.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn stop_office_watch(root_path: String, path: String) -> Result<(), AppCommandError> {
    crate::office_watch::stop_office_watch_core(
        root_path,
        path,
        crate::office_watch::WatchOrigin::Desktop,
    )
    .await
    .map_err(Into::into)
}
