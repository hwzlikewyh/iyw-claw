use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant};

/// Windows 的 Chromium 启动进程可能早于实际写文件的进程返回。
/// 等待同目录暂存文件完整后发布，以免读到旧文件或未写完的内容。
pub(super) struct RenderOutput(tempfile::TempPath);

impl RenderOutput {
    pub fn new(destination: &str, suffix: &str) -> Result<Self, String> {
        let dest = std::path::absolute(destination).map_err(|e| e.to_string())?;
        let parent = dest.parent().ok_or("invalid output directory")?;
        let staged = tempfile::Builder::new()
            .prefix("office-preview-")
            .suffix(suffix)
            .tempfile_in(parent)
            .map_err(|e| e.to_string())?;
        Ok(Self(staged.into_temp_path()))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn finish(self, destination: &str, footer: &[u8]) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if self.has_footer(footer) {
                return self.0.persist(destination).map_err(|e| e.to_string());
            }
            if Instant::now() >= deadline {
                return Err("browser produced no complete output before timeout".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn has_footer(&self, footer: &[u8]) -> bool {
        let Ok(mut file) = std::fs::File::open(self.path()) else {
            return false;
        };
        let mut tail = [0; 16];
        if file.seek(SeekFrom::End(-16)).is_err() || file.read_exact(&mut tail).is_err() {
            return false;
        }
        tail.windows(footer.len()).any(|part| part == footer)
    }
}
