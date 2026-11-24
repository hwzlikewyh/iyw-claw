import { readFile } from "node:fs/promises"

function replaceOnce(text, before, after) {
  if (text.split(before).length !== 2) throw new Error("Word pagination integration point changed")
  return text.replace(before, () => after)
}

// 采用固定版本的 OfficeCLI 排版算法，在当前预览的 Shadow DOM 内测量页面。
export async function wordPagination(source) {
  const layout = await source("crates/hcd-core/src/docx_layout.js")
  const pager = await source("crates/hcd-core/src/docx_pagination.js")
  const presentation = await source("crates/hcd-core/src/presentation.rs")
  const css = presentation.match(/const DOCX_VIRTUAL_PAGE_CSS: &str = r#"([\s\S]*?)"#;/)?.[1]
  const start = pager.indexOf("  const view = doc.defaultView;")
  const end = pager.indexOf("  const ready = () =>")
  if (!css || start < 0 || end <= start) throw new Error("Word pagination source changed")
  const metrics = await readFile(new URL("../../src-tauri/vendor/officecli-embedded/assets/word-line-metrics.js", import.meta.url), "utf8")
  const rows = await readFile(new URL("../../src-tauri/vendor/officecli-embedded/assets/word-row-split.js", import.meta.url), "utf8")
  const keepNext = await readFile(new URL("../../src-tauri/vendor/officecli-embedded/assets/word-keep-next.js", import.meta.url), "utf8")
  const fonts = await readFile(new URL("../../src-tauri/vendor/officecli-embedded/assets/office-fonts.js", import.meta.url), "utf8")
  let paginate = replaceOnce(pager.slice(start, end), "doc.head.append(print);", "head.append(print);")
  paginate = replaceOnce(paginate, "if (rows.length < 2)", "if (!rows.length)")
  paginate = replaceOnce(paginate, "    const tail = table.cloneNode(false);",
    "    freezeWordTable(table);\n    const tail = table.cloneNode(false);")
  paginate = replaceOnce(paginate, "const keep = ['avoid','avoid-page'].includes(style.breakInside);",
    "const keep = node.dataset.hcdKeepLines === 'true' || ['avoid','avoid-page'].includes(style.breakInside);")
  paginate = replaceOnce(paginate, "      add(block.node);", `
      // Word 的强制分页段落到达页首时不再叠加段前间距。
      if (!count && pages.length > 1 && block.before && block.node.matches('p,h1,h2,h3,h4,h5,h6')) {
        block.node.style.marginTop = '0';
      }
      add(block.node);
      if (count > 1 && fits() && !wordKeepNextFits(blocks, index, content)) {
        block.node.remove(); count--; newPage(block.layout, block.section, false); add(block.node);
      }`)
  paginate = replaceOnce(paginate, "for (const group of groups.slice(i)) remainder.append(...group);", `
        const rest = groups[i].length === 1 && splitWordRow(groups[i][0], content, fits);
        if (rest) { remainder.append(rest); accepted++; }
        for (const group of groups.slice(rest ? i + 1 : i)) remainder.append(...group);`)
  return `${fonts}
const wordPageCss = ${JSON.stringify(css + "\nbody.hcd-docx-paginated{align-items:safe center}")};
${layout.replace("export function applyWordGrid", "function applyWordGrid")}
function paginateWord(body, head) {
  const doc = body.ownerDocument;
  ${metrics}
  ${rows}
  ${keepNext}
  ${paginate}
  paginate();
}`
}
