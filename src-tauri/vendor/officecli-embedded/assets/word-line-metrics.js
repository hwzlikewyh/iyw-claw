function applyWordAutoSpacing(body) {
  const context = body.ownerDocument.createElement('canvas').getContext('2d');
  if (!context) return;
  const heights = new Map();
  // Windows WPS 12 的单倍行距，来自同字号三行 DOCX 的基线间距 / 字号。
  const referenceEm = new Map([['noto sans sc', 1.88], ['simsun', 1.30], ['宋体', 1.30]]);
  function fontHeight(style) {
    const font = style.fontStyle + ' ' + style.fontWeight + ' ' + style.fontSize + ' ' + style.fontFamily;
    if (!heights.has(font)) {
      context.font = font;
      const metric = context.measureText('国Ag');
      const family = style.fontFamily.split(',')[0].trim().replace(/["']/g, '').toLowerCase();
      const measured = metric.fontBoundingBoxAscent + metric.fontBoundingBoxDescent;
      const single = referenceEm.get(family);
      heights.set(font, single && hasWindowsWordFace(context, family) ? parseFloat(style.fontSize) * single : measured);
    }
    return heights.get(font);
  }
  for (const paragraph of body.querySelectorAll('.hcd-paragraph')) {
    const style = getComputedStyle(paragraph);
    if (style.getPropertyValue('--hcd-line-rule').trim() !== 'auto') continue;
    const multiple = parseFloat(style.lineHeight) / parseFloat(style.fontSize);
    if (!Number.isFinite(multiple) || multiple <= 0) continue;
    // Word 的自动行距以实际字体的一行高度为基数。每个字号只影响其所在的行。
    for (const run of [paragraph, ...paragraph.querySelectorAll('.hcd-run')]) {
      const height = fontHeight(getComputedStyle(run));
      if (height > 0) run.style.lineHeight = (height * multiple) + 'px';
    }
  }
}
applyWordAutoSpacing(body);
// 本机 WPS 对 Noto Sans SC 可变字体选用 Thin 实例，粗体另以约 1/35 em 描边。
// 仅在 Windows 能解析该字体实例时匹配这项行为。
applyWindowsNotoFace(body);
// 普通 Word 表格直接接在段落之后，段落的段后距与单元格边距已在导入时还原。
for (const table of body.querySelectorAll('table.hcd-table')) {
  if (!table.style.marginTop) table.style.marginTop = '0';
  if (!table.style.marginBottom) table.style.marginBottom = '0';
  // 自动布局的 tblGrid 是初始网格，不是强制列宽；单元格显式宽度仍由原样式保留。
  if (table.style.tableLayout !== 'fixed') {
    for (const column of table.querySelectorAll(':scope>colgroup>col')) column.style.width = '';
  }
}
