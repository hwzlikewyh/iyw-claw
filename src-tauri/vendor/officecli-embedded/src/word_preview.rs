use std::io::{self, Write};
use std::path::Path;

use hcd_core::{Bundle, HtmlPresentationOptions};
use hcd_formats::ImportOptions;

const PAGE_SCRIPT: &[u8] = b"<script id=\"hcd-docx-virtual-pages\" type=\"module\">";
const WORD_PAGES: &[u8] = include_bytes!("../assets/word-pages.js");
const START: &[u8] = br#"
function renderPages() {
    try {
        paginateWord(document.body, document.head);
    } catch (error) { document.body.dataset.hcdPaginationError = String(error); }
}
if (document.readyState === 'complete') queueMicrotask(renderPages);
else window.addEventListener('load', renderPages, { once: true });
</script>"#;

/// 固定版本的 HCD 导出器将内置分页模块作为一个独立的写入块发送。
/// 在它测量页面前应用 Word 字体行框，其余输出继续流式写入。
pub struct WordLayoutWriter<W>(pub W);

impl<W: Write> Write for WordLayoutWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes == b"</body></html>" {
            self.0.write_all(b"<script>")?;
            self.0
                .write_all(include_bytes!("../assets/office-fonts.js"))?;
            self.0
                .write_all(include_bytes!("../assets/slide-layout.js"))?;
            self.0
                .write_all(include_bytes!("../assets/grid-layout.js"))?;
            self.0
                .write_all(b"standaloneSlides(document.body,document.head);</script>")?;
        }
        if !bytes.starts_with(PAGE_SCRIPT) {
            return self.0.write(bytes);
        }
        self.0.write_all(PAGE_SCRIPT)?;
        self.0.write_all(WORD_PAGES)?;
        self.0.write_all(START)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

pub(crate) fn render(file: &Path) -> anyhow::Result<String> {
    let temp = tempfile::tempdir()?;
    let target = temp.path().join("bundle");
    hcd_formats::import_document(file, &target, &ImportOptions::new("preview"), |_| Ok(()))?;
    crate::preview_styles::write(file, &target)?;
    let options = HtmlPresentationOptions {
        text_hitboxes_enabled: false,
        image_hitboxes_enabled: false,
        override_stylesheet: Some(crate::preview_styles::read(&target)?),
        ..Default::default()
    };
    let mut output = Vec::new();
    office_engine::render_hcd_html(
        &Bundle::open(&target)?,
        &options,
        &mut WordLayoutWriter(&mut output),
    )?;
    Ok(String::from_utf8(output)?)
}
