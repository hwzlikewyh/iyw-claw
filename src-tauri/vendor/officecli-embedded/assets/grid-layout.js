// 独立 HTML 与 PDF 按原样式缩字；只改变派生视图，不重写单元格内容。
function fitSpreadsheet(body) {
  if (body.dataset.hcdSourceFormat !== "xlsx") return;
  for (const sheet of body.querySelectorAll('.hcd-grid-sheet-content')) {
    const display = sheet.style.display;
    if (!sheet.clientWidth) sheet.style.display = 'block';
    fixSpreadsheetWidths(sheet);
    for (const cell of sheet.querySelectorAll('td[data-hcd-cell]')) {
      const style = getComputedStyle(cell);
      if (style.getPropertyValue('--hcd-shrink-to-fit').trim() !== '1') continue;
      if (['normal','pre-wrap','pre-line','break-spaces'].includes(style.whiteSpace)) continue;
      const range = cell.ownerDocument.createRange();
      range.selectNodeContents(cell);
      const text = range.getBoundingClientRect().width;
      const available = cell.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      if (text > available && available > 0) {
        cell.style.fontSize = Math.max(1, parseFloat(style.fontSize) * available / text) + 'px';
      }
    }
    sheet.style.display = display;
  }
}

function fixSpreadsheetWidths(sheet) {
  // table-layout:fixed 只有指定总宽度才生效；否则长文字会把列和背景网格挤错位。
  for (const table of sheet.querySelectorAll('table.hcd-grid')) {
    let width = 0;
    for (const column of table.querySelectorAll(':scope>colgroup>col')) {
      if (getComputedStyle(column).display === 'none') continue;
      const size = parseFloat(column.style.width || getComputedStyle(column).width);
      if (Number.isFinite(size)) width += size * column.span;
    }
    if (width > 0) table.style.width = width + 'px';
  }
}

if (document.body.dataset.hcdSourceFormat === 'xlsx') {
  window.addEventListener('load', () => fitSpreadsheet(document.body), { once: true });
  window.addEventListener('beforeprint', () => fitSpreadsheet(document.body));
}
