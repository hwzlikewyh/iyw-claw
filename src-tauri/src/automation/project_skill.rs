use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::db::service::folder_service;
use crate::db::AppDatabase;
use crate::models::{AgentType, AutomationConfig, AutomationInfo};

struct SkillTarget {
    path: PathBuf,
    marker: String,
}

struct SkillDocument<'a> {
    automation: &'a AutomationInfo,
    config: &'a AutomationConfig,
    result_summary: Option<&'a str>,
    marker: &'a str,
}

pub async fn persist(
    db: &AppDatabase,
    automation: &AutomationInfo,
    run_id: i32,
    result_summary: Option<&str>,
) -> Result<bool, String> {
    let target = resolve_target(db, automation).await?;
    if skill_is_current(&target) {
        return Ok(false);
    }
    let config: AutomationConfig = serde_json::from_value(automation.config.clone())
        .map_err(|error| format!("invalid automation config: {error}"))?;
    let content = build_content(SkillDocument {
        automation,
        config: &config,
        result_summary,
        marker: &target.marker,
    });
    write_direct(&target.path, &content).map_err(|error| error.to_string())?;
    tracing::info!(
        automation_id = automation.id,
        run_id,
        path = %target.path.display(),
        "[automation] project skill written"
    );
    Ok(true)
}

async fn resolve_target(
    db: &AppDatabase,
    automation: &AutomationInfo,
) -> Result<SkillTarget, String> {
    let folder_id = automation
        .root_folder_id
        .ok_or_else(|| "automation has no root folder".to_string())?;
    let folder = folder_service::get_folder_by_id(&db.conn, folder_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("automation folder {folder_id} is missing"))?;
    let agent_type = parse_agent_type(&automation.agent_type)?;
    let skill_root = crate::commands::acp::skill_storage_spec(agent_type)
        .and_then(|spec| spec.project_rel_dirs.first().copied())
        .unwrap_or(".agents/skills");
    Ok(SkillTarget {
        path: PathBuf::from(folder.path)
            .join(skill_root)
            .join(format!("automation-{}", automation.id))
            .join("SKILL.md"),
        marker: source_marker(automation),
    })
}

fn build_content(document: SkillDocument<'_>) -> String {
    let prompt = prompt_text(document.config);
    let summary = document
        .result_summary
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("本次自动化已成功执行，但没有可提取的结果摘要。");
    let description = skill_description(&document.automation.name, &prompt);
    format!(
        "---\nname: automation-{id}\ndescription: \"{description}\"\n---\n\n\
         <!-- iyw-claw-automation: id={id} {marker} -->\n\n# {name}\n\n\
         ## 执行流程\n\n{prompt}\n\n## 成功执行参考\n\n{summary}\n",
        id = document.automation.id,
        description = yaml_scalar(&description),
        name = document.automation.name,
        marker = document.marker,
        prompt = truncate(&prompt, 12_000),
        summary = truncate(summary, 12_000),
    )
}

fn prompt_text(config: &AutomationConfig) -> String {
    if !config.display_text.trim().is_empty() {
        return config.display_text.clone();
    }
    config
        .prompt_blocks
        .iter()
        .filter_map(|block| block.get("text").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

fn skill_description(name: &str, prompt: &str) -> String {
    let prompt = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "用于执行当前项目的自动化“{}”：{}",
        name.trim(),
        truncate(&prompt, 180)
    )
}

fn source_marker(automation: &AutomationInfo) -> String {
    let source = format!(
        "{}\0{}\0{}",
        automation.name, automation.agent_type, automation.config
    );
    format!("source_sha256={:x}", Sha256::digest(source.as_bytes()))
}

fn skill_is_current(target: &SkillTarget) -> bool {
    fs::read_to_string(&target.path)
        .ok()
        .is_some_and(|content| content.contains(&target.marker))
}

fn write_direct(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)
}

fn parse_agent_type(value: &str) -> Result<AgentType, String> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|_| format!("unknown agent type: {value}"))
}

fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn yaml_scalar(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}
