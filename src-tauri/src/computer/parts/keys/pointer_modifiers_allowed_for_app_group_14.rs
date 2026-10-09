// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// [`pointer_modifiers_allowed`] for a grant on the whole application: Option
/// too on a Mac — what it reaches beyond the window is the application's —
/// and still never the Windows / Super key elsewhere.
pub fn pointer_modifiers_allowed_for_app(modifiers: Modifiers, platform: Platform) -> bool {
    !modifiers.meta || platform == Platform::Mac
}

/// Whether `chord` copies or cuts: the platform's shortcut modifier with C
/// or X — what is watched for a change of the clipboard.
pub fn copies(chord: &Chord, platform: Platform) -> bool {
    let primary = match platform {
        Platform::Mac => chord.modifiers.meta,
        Platform::Windows | Platform::Linux => chord.modifiers.control,
    };
    primary && matches!(chord.key, Key::Char('c' | 'x'))
}
