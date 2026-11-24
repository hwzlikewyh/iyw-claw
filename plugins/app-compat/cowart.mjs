import { readFile, writeFile } from "node:fs/promises"
import path from "node:path"
import { replaceOnce } from "./patches.mjs"

export async function repairCowart(root) {
  const runtime = path.join(root, "mcp/generated/cowart-mcp.mjs")
  let server = await readFile(runtime, "utf8")
  server = replaceOnce(
    server,
    "globalThis.__COWART_MCP_APP__ = mcpApp;",
    `
    globalThis.__COWART_MCP_APP__ = mcpApp;
    mcpApp.onteardown = async () => {
      await window.__cowartFlush?.();
      return {};
    };`
  )
  await writeFile(runtime, server)
  const file = path.join(root, "mcp/generated/cowart-widget.html")
  await writeFile(file, patchWidget(await readFile(file, "utf8")))
}

function patchWidget(html) {
  html = replaceOnce(
    html,
    "async function lt(){if(!P)return;",
    `
let iywSave = null;
function lt(strict = false) {
  if (iywSave) return strict ? iywSave.then(() => lt(true)) : iywSave;
  const task = iywPersist(strict);
  iywSave = task;
  return task.finally(() => { if (iywSave === task) iywSave = null; });
}
window.__cowartFlush = async () => {
  document.activeElement?.blur();
  await Promise.all([...zS.values()].map(editor => editor.flush()));
  // 等待 tldraw 按帧分发的变更通知，将最后一次编辑纳入保存队列。
  await new Promise(resolve => requestAnimationFrame(resolve));
  while (P || iywSave) await lt(true);
};
async function iywPersist(iywStrict){if(!P)return;`
  )
  html = replaceOnce(
    html,
    "P=Q!==dt}catch(Vt){console.error(Vt)}finally{q=!1,(D||P)&&(D=!1,M())}",
    "P=Q!==dt}catch(Vt){console.error(Vt);if(iywStrict)throw Vt}finally{q=!1,(D||P)&&(D=!1,M())}"
  )
  html = replaceOnce(
    html,
    "delete window.__cowartEditor,delete window.__cowartSelection",
    "delete window.__cowartFlush,delete window.__cowartEditor,delete window.__cowartSelection"
  )
  return html
}
