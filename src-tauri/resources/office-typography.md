## iyw-claw managed office typography

Apply these rules to Office documents (PPTX, DOCX, XLSX), HTML and PDF deliverables, regardless of the generator: OfficeCLI, PptxGenJS, python-pptx, python-docx, openpyxl or another tool. Font pairings and command examples in other Skills are illustrative; replace their fonts with the licensed choices below.

### Font choice and licensing

- Default Chinese and Latin text to `Noto Sans SC` (SIL Open Font License 1.1, permits free commercial use). `Source Han Sans SC` / 思源黑体 is an alternative when that exact family is installed and its license is verified.
- Do not default to `Microsoft YaHei`, `Microsoft YaHei UI`, 微软雅黑, 宋体, 黑体, 等线, `Calibri`, `Arial`, `Georgia`, `Segoe UI` or other proprietary system fonts. Being installed with Windows or Office does not prove permission to redistribute or embed a font, or that it meets a free-commercial-use requirement.
- Preserve an existing template's layout. If the user requires free commercial use, replace conflicting template fonts too; a template is not a font-license exception. Honor an explicitly named user font, but do not claim that an unverified license permits free commercial use.
- Before rendering, confirm the selected family is available to the renderer. If unavailable, select an installed font with a verified free commercial license or report the rendering limitation. Never silently fall back to Microsoft YaHei or claim a font was rendered based only on its name in the file.
- Redistribute or embed font files only as their license permits, with the required license notices. For other writing systems, choose a licensed family covering the actual characters.

### Write explicit fonts and document defaults

- PPTX: set `font.latin=Noto Sans SC` and `font.ea=Noto Sans SC` on new text shapes, runs and table-cell text. Set chart titles, axes, legends and labels with their supported font properties; consult current help instead of assuming shape properties work on charts. Set both heading/body Latin and East Asian theme fonts; inspect all themes, masters, layouts and notes for inherited or explicit conflicting fonts.
- DOCX: set `font.latin=Noto Sans SC` and `font.ea=Noto Sans SC` on new paragraphs, styles, runs and table-cell text. Also set `docDefaults.font`, `docDefaults.font.hAnsi` and `docDefaults.font.eastAsia`, plus both heading/body Latin and East Asian theme fonts. Inspect styles, font tables and headers/footers for conflicting fonts or theme references.
- XLSX: set `font=Noto Sans SC` on created cells, including numeric cells and table headers. Set both heading/body Latin and East Asian theme fonts and inspect workbook styles, drawings and charts. Creating cells with the right font still leaves the initial font record in `xl/styles.xml`; replace that record too, using raw XML if the current CLI has no default-font setter. A sheet preview alone does not prove all cell fonts are correct.
- HTML: prefer `font-family: "Noto Sans SC", "Source Han Sans SC", sans-serif`; do not name proprietary fonts in fallback lists.

OfficeCLI examples (replace the filename with the actual output):

```bash
officecli set output.pptx / --prop "theme.font.major.latin=Noto Sans SC" --prop "theme.font.major.eastAsia=Noto Sans SC" --prop "theme.font.minor.latin=Noto Sans SC" --prop "theme.font.minor.eastAsia=Noto Sans SC"
officecli set output.docx / --prop "docDefaults.font=Noto Sans SC" --prop "docDefaults.font.hAnsi=Noto Sans SC" --prop "docDefaults.font.eastAsia=Noto Sans SC"
officecli set output.docx / --prop "theme.font.major.latin=Noto Sans SC" --prop "theme.font.major.eastAsia=Noto Sans SC" --prop "theme.font.minor.latin=Noto Sans SC" --prop "theme.font.minor.eastAsia=Noto Sans SC"
officecli set output.xlsx / --prop "theme.font.major.latin=Noto Sans SC" --prop "theme.font.major.eastAsia=Noto Sans SC" --prop "theme.font.minor.latin=Noto Sans SC" --prop "theme.font.minor.eastAsia=Noto Sans SC"
officecli raw-set output.xlsx /xl/styles.xml --xpath "/x:styleSheet/x:fonts/x:font/x:name" --action setattr --xml "val=Noto Sans SC"
```

### Font delivery gate

Before delivery, flush/close the editor, enumerate the font declarations inside the actual saved file, and verify the selected families and licenses. For Office files, inspect the OOXML ZIP parts, including themes, masters/layouts, styles, font tables, charts, notes and embedded workbooks. Check actual font attributes (`typeface`, `w:rFonts`, spreadsheet font `name`), not ordinary prose mentioning a font.

When free commercial use is required, any conflicting font declaration must be corrected before delivery. Use supported DOM edits first and raw XML only where necessary, then run the format's normal validation and recheck the saved package. A changed preview, default font or a verbal promise is insufficient. If the package or rendering cannot be checked, state that specific verification gap instead of claiming full compliance.

<!-- iyw-claw managed office typography end -->
