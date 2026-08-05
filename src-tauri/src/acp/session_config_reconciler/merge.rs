//! 有效值来源优先级合并：backend emergency kill switch > backend mandatory
//! org policy > user explicit preference > migrated legacy preference >
//! product default。纯函数，供 fixture 测试。

use serde::{Deserialize, Serialize};

/// 有效开关的来源。UI 必须如实展示，不能伪装成用户设置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveSource {
    /// 后台安全 kill switch（管理员紧急关闭）。
    KillSwitch,
    /// 后台强制组织策略。
    OrgPolicy,
    /// 用户显式偏好。
    UserPreference,
    /// 迁移自旧持久键的一次性默认。
    Migrated,
    /// 产品默认值。
    ProductDefault,
}

/// 合并后的有效开关值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveFlag {
    pub enabled: bool,
    pub source: EffectiveSource,
}

/// 按固定优先级合并四类输入，返回有效开关与来源。
///
/// `kill_switch` 与 `org_policy` 为后台策略（`Some` 即生效）；
/// `user` 为用户显式偏好；`migrated` 为迁移自旧键的一次性默认；
/// `product_default` 兜底。优先级高的来源存在时直接采用。
pub fn resolve_feature_flag(
    kill_switch: Option<bool>,
    org_policy: Option<bool>,
    user: Option<bool>,
    migrated: Option<bool>,
    product_default: bool,
) -> EffectiveFlag {
    if let Some(enabled) = kill_switch {
        return EffectiveFlag {
            enabled,
            source: EffectiveSource::KillSwitch,
        };
    }
    if let Some(enabled) = org_policy {
        return EffectiveFlag {
            enabled,
            source: EffectiveSource::OrgPolicy,
        };
    }
    if let Some(enabled) = user {
        return EffectiveFlag {
            enabled,
            source: EffectiveSource::UserPreference,
        };
    }
    if let Some(enabled) = migrated {
        return EffectiveFlag {
            enabled,
            source: EffectiveSource::Migrated,
        };
    }
    EffectiveFlag {
        enabled: product_default,
        source: EffectiveSource::ProductDefault,
    }
}

/// 产品默认值：新安装 delegation / feedback 均默认开启。
pub const PRODUCT_DEFAULT_DELEGATION_ENABLED: bool = true;
pub const PRODUCT_DEFAULT_FEEDBACK_ENABLED: bool = true;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_switch_wins_over_everything() {
        let flag = resolve_feature_flag(Some(false), Some(true), Some(true), Some(true), true);
        assert_eq!(
            flag,
            EffectiveFlag {
                enabled: false,
                source: EffectiveSource::KillSwitch,
            }
        );
    }

    #[test]
    fn org_policy_beats_user_and_default() {
        let flag = resolve_feature_flag(None, Some(true), Some(false), None, false);
        assert_eq!(
            flag,
            EffectiveFlag {
                enabled: true,
                source: EffectiveSource::OrgPolicy,
            }
        );
    }

    #[test]
    fn user_preference_wins_over_migrated_and_default() {
        let flag = resolve_feature_flag(None, None, Some(false), Some(true), true);
        assert_eq!(
            flag,
            EffectiveFlag {
                enabled: false,
                source: EffectiveSource::UserPreference,
            }
        );
    }

    #[test]
    fn migrated_beats_product_default() {
        let flag = resolve_feature_flag(None, None, None, Some(true), false);
        assert_eq!(
            flag,
            EffectiveFlag {
                enabled: true,
                source: EffectiveSource::Migrated,
            }
        );
    }

    #[test]
    fn product_default_is_last_resort() {
        let flag = resolve_feature_flag(None, None, None, None, true);
        assert_eq!(
            flag,
            EffectiveFlag {
                enabled: true,
                source: EffectiveSource::ProductDefault,
            }
        );
    }

    #[test]
    fn new_install_defaults_are_on() {
        assert!(PRODUCT_DEFAULT_DELEGATION_ENABLED);
        assert!(PRODUCT_DEFAULT_FEEDBACK_ENABLED);
    }
}
