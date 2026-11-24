// 在移走任何行之前固定完整表格的实际列宽，避免每个续页按剩余内容重新分配列宽。
function freezeWordTable(table) {
  if (table.style.tableLayout === 'fixed') return;
  const columns = [...table.querySelectorAll(':scope>colgroup>col')];
  if (!columns.length) return;
  const widths = columns.map(column => column.getBoundingClientRect().width / column.span);
  if (widths.some(width => !Number.isFinite(width) || width <= 0)) return;
  columns.forEach((column, index) => { column.style.width = widths[index] + 'px'; });
  table.style.tableLayout = 'fixed';
}

// 找出超过正文底边的第一行。Range 使用实际浏览器断行，保留行内格式和链接。
function wordCellCut(cell, bottom) {
  const range = cell.ownerDocument.createRange();
  const walker = cell.ownerDocument.createTreeWalker(cell, NodeFilter.SHOW_TEXT);
  for (let text = walker.nextNode(); text; text = walker.nextNode()) {
    if (!text.length) continue;
    range.selectNodeContents(text);
    if (range.getBoundingClientRect().bottom <= bottom) continue;
    const chars = Array.from(text.data);
    const offsets = [0];
    for (const ch of chars) offsets.push(offsets.at(-1) + ch.length);
    let low = 0, high = chars.length;
    while (low < high) {
      const mid = Math.floor((low + high) / 2);
      range.setStart(text, offsets[mid]);
      range.setEnd(text, offsets[mid + 1]);
      if (range.getBoundingClientRect().bottom > bottom) high = mid;
      else low = mid + 1;
    }
    if (low < chars.length) return { text, offset: offsets[low] };
  }
  return null;
}

function splitWordRow(row, content, fits) {
  if (row.dataset.hcdRepeatHeader === 'true' || /avoid/.test(getComputedStyle(row).breakInside)) return null;
  if (row.querySelector('table,img,svg,canvas,br[data-hcd-break]') || row.style.height) return null;
  if ([...row.cells].some(cell => cell.rowSpan !== 1)) return null;
  const bottom = content.getBoundingClientRect().bottom;
  const original = row.cloneNode(true);
  let boundary = bottom;
  // 字形底边与实际行框底边可能不同。按试排后溢出的高度收回最后一行。
  for (let attempt = 0; attempt < 2; attempt++) {
    const tail = takeWordRowTail(row, boundary);
    if (!tail) return null;
    if (row.textContent.trim() && tail.textContent.trim() && fits()) return tail;
    const overflow = content.scrollHeight - content.clientHeight;
    row.replaceChildren(...original.cloneNode(true).childNodes);
    if (overflow <= 0) return null;
    boundary -= overflow + 1;
  }
  return null;
}

function takeWordRowTail(row, bottom) {
  const cells = [...row.cells];
  const cuts = cells.map(cell => wordCellCut(cell, bottom - parseFloat(getComputedStyle(cell).paddingBottom)));
  if (!cuts.some(Boolean)) return null;
  const tail = row.cloneNode(false);
  cells.forEach((cell, index) => {
    const copy = cell.cloneNode(false), cut = cuts[index];
    tail.append(copy);
    if (!cut) return;
    const range = cell.ownerDocument.createRange();
    range.selectNodeContents(cell);
    range.setStart(cut.text, cut.offset);
    copy.append(range.extractContents());
    const continuation = copy.firstElementChild;
    if (continuation && cut.offset > 0) {
      continuation.style.marginTop = '0';
      continuation.style.textIndent = '0';
    }
  });
  return tail;
}
