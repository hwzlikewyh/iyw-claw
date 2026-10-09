// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Modifiers {
    /// Parse an agent's list of modifier names.
    pub fn parse<S: AsRef<str>>(names: &[S]) -> Result<Modifiers, String> {
        let mut m = Modifiers::default();
        for name in names {
            match name.as_ref().trim().to_ascii_lowercase().as_str() {
                "shift" => m.shift = true,
                "control" | "ctrl" => m.control = true,
                "alt" | "option" | "opt" => m.alt = true,
                "meta" | "command" | "cmd" | "super" | "win" | "windows" => m.meta = true,
                other => {
                    return Err(format!(
                        "`{other}` is not a modifier. Modifiers: shift, control, alt (option), \
                         meta (command on a Mac)"
                    ))
                }
            }
        }
        Ok(m)
    }

    pub fn is_empty(self) -> bool {
        self == Modifiers::default()
    }

    /// The modifiers as `platform`'s driver spells them.
    pub fn driver_names(self, platform: Platform) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.shift {
            out.push("shift");
        }
        if self.control {
            out.push("ctrl");
        }
        if self.alt {
            out.push(match platform {
                Platform::Mac => "option",
                Platform::Windows | Platform::Linux => "alt",
            });
        }
        if self.meta {
            out.push(match platform {
                Platform::Mac => "cmd",
                Platform::Windows => "win",
                Platform::Linux => "super",
            });
        }
        out
    }
}
