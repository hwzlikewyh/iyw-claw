// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The words of a launch command as a shell would split them: on white
/// space, a double-quoted stretch kept whole (its quotes dropped). Enough to
/// find the programs and the names a command carries — the executable, a
/// wrapper's application id (`flatpak run org.keepassxc.KeePassXC`) — not to
/// run it.
pub fn command_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    for c in command.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}
