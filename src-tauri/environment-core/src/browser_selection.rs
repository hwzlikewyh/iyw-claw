use anyhow::{bail, Result};

use crate::model::{EnvironmentPlan, EnvironmentSnapshot};
use crate::paths::Layout;

const BROWSER_COMPONENTS: [&str; 2] = ["chromix", "agent-browser"];

pub fn apply(
    plan: &mut EnvironmentPlan,
    layout: &Layout,
    current: Option<&EnvironmentSnapshot>,
    repair: Option<&[String]>,
) -> Result<()> {
    let explicit = repair.is_some_and(|components| {
        components
            .iter()
            .any(|id| BROWSER_COMPONENTS.contains(&id.as_str()))
    });
    let enabled = std::fs::read(layout.root.join("preferences.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
        .is_some_and(|value| value["full_browser_enabled"] == true);
    let installed = current.is_some_and(|snapshot| {
        snapshot
            .components
            .iter()
            .any(|component| BROWSER_COMPONENTS.contains(&component.component_id.as_str()))
    });
    if explicit {
        for id in BROWSER_COMPONENTS {
            if !plan.actions.iter().any(|action| action.component_id == id) {
                bail!("完整浏览器组件尚未发布：{id}")
            }
        }
    }
    // 不篡改 Fusion 制品；只从已经验证的安装计划中选择本机要安装的组件。
    if !explicit && !enabled && !installed {
        plan.actions
            .retain(|action| !BROWSER_COMPONENTS.contains(&action.component_id.as_str()));
    }
    Ok(())
}
