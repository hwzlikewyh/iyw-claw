// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The default blocklist: applications whose windows are credentials, and the
/// switches that decide who else may read the screen. Matched against a
/// bundle identifier, a full path, or an executable's file name, without
/// regard to case, on whichever platform names them.
///
/// Every entry is the person's to take off: none is beyond an agent's reach
/// for want of a way — a window of any of them can be read and operated like
/// any other — only for what it holds, and the person decides that.
pub const DEFAULT_BLOCKLIST: &[DefaultBlock] = &[
    // System Settings holds the Privacy & Security pane that decides who may
    // record the screen, this helper included.
    DefaultBlock {
        key: "system-settings",
        name: "System Settings",
        macos: &["com.apple.systempreferences", "com.apple.Settings"],
        windows: &["systemsettings.exe"],
        linux: &[],
    },
    // The system's own prompts for a password (an administrator password
    // typed into one is exactly what must not reach a screenshot) and for
    // elevation.
    DefaultBlock {
        key: "credential-prompts",
        name: "Password prompts",
        macos: &[
            "com.apple.SecurityAgent",
            "com.apple.LocalAuthentication.UIAgent",
        ],
        windows: &["credentialuibroker.exe", "consent.exe"],
        linux: &[],
    },
    DefaultBlock {
        key: "keychain",
        name: "Keychain Access",
        macos: &["com.apple.keychainaccess"],
        windows: &[],
        linux: &[],
    },
    DefaultBlock {
        key: "seahorse",
        name: "Passwords and Keys",
        macos: &[],
        windows: &[],
        linux: &["seahorse"],
    },
    DefaultBlock {
        key: "kwallet",
        name: "KWallet Manager",
        macos: &[],
        windows: &[],
        linux: &["kwalletmanager5"],
    },
    DefaultBlock {
        key: "passwords",
        name: "Passwords",
        macos: &["com.apple.Passwords"],
        windows: &[],
        linux: &[],
    },
    DefaultBlock {
        key: "1password",
        name: "1Password",
        macos: &["com.1password.1password", "com.agilebits.onepassword7"],
        windows: &["1password.exe"],
        linux: &["1password"],
    },
    DefaultBlock {
        key: "bitwarden",
        name: "Bitwarden",
        macos: &["com.bitwarden.desktop"],
        windows: &["bitwarden.exe"],
        linux: &["bitwarden"],
    },
    DefaultBlock {
        key: "keepass",
        name: "KeePass",
        macos: &[],
        windows: &["keepass.exe"],
        linux: &[],
    },
    DefaultBlock {
        key: "keepassxc",
        name: "KeePassXC",
        macos: &["org.keepassxc.keepassxc"],
        windows: &["keepassxc.exe"],
        linux: &["keepassxc"],
    },
    DefaultBlock {
        key: "lastpass",
        name: "LastPass",
        macos: &["com.lastpass.LastPass"],
        windows: &[],
        linux: &[],
    },
    DefaultBlock {
        key: "enpass",
        name: "Enpass",
        macos: &["in.sinew.Enpass-Desktop"],
        windows: &["enpass.exe"],
        linux: &[],
    },
    DefaultBlock {
        key: "proton-pass",
        name: "Proton Pass",
        macos: &["me.proton.pass.electron"],
        windows: &["proton pass.exe"],
        linux: &[],
    },
    DefaultBlock {
        key: "dashlane",
        name: "Dashlane",
        macos: &["com.dashlane.dashlanephonefinal"],
        windows: &["dashlane.exe"],
        linux: &[],
    },
];

/// One default entry as the settings show it on this platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultBlockView {
    pub key: String,
    pub name: String,
    /// How this platform names it.
    pub names: Vec<String>,
}

/// The default entries this platform has a name for, in list order.
pub fn default_blocklist(platform: Platform) -> Vec<DefaultBlockView> {
    DEFAULT_BLOCKLIST
        .iter()
        .filter(|block| !block.names_on(platform).is_empty())
        .map(|block| DefaultBlockView {
            key: block.key.to_string(),
            name: block.name.to_string(),
            names: block
                .names_on(platform)
                .iter()
                .map(|name| name.to_string())
                .collect(),
        })
        .collect()
}

/// Whether `key` names a default entry — one a person may take off the list.
pub fn is_default_key(key: &str) -> bool {
    DEFAULT_BLOCKLIST.iter().any(|block| block.key == key)
}

/// The applications whose windows can never be shared: the default list,
/// less the entries the person took off it, plus whatever they added.
///
/// A person may take a default off — theirs to decide, from the settings. An
/// agent that edits those settings behind them could do the same; what it
/// cannot do is share a window: that still takes the person, in the picker.
#[derive(Debug, Clone, Default)]
pub struct Blocklist {
    /// Lowercased.
    pub(in crate::computer::agent) entries: Vec<String>,
}
