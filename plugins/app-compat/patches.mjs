import { readFile, writeFile } from "node:fs/promises"
import path from "node:path"

export function replaceOnce(source, before, after) {
  if (source.split(before).length !== 2)
    throw Error("Published plugin source changed; review the patch target")
  return source.replace(before, () => after)
}

function appendScript(html, script) {
  return replaceOnce(html, "</body>", `<script>${script}</script>\n</body>`)
}

export function addOfficeTeardown(html) {
  return appendScript(
    html,
    `
// 关闭前释放当前预览会话；协议错误交给宿主处理。
window.__officeHost.app.onteardown = async () => {
  const host = window.__officeHost;
  const result = await host.app.callServerTool({
    name: "release_view", arguments: { viewId: host.viewId }
  });
  if (result.isError) throw Error("Could not release the document preview");
  return {};
};`
  )
}

export async function repairOffice(root) {
  const file = path.join(root, "source/dist/ui/panel.html")
  await writeFile(file, addOfficeTeardown(await readFile(file, "utf8")))
}

export async function repairExcalidraw(root) {
  const files = ["source/dist/server.mjs", "source/dist/ui/bundle-6.txt"]
  for (const file of files) {
    const target = path.join(root, file)
    await writeFile(
      target,
      replaceOnce(
        await readFile(target, "utf8"),
        '"excalidraw://document/"',
        '"ui://excalidraw/document/"'
      )
    )
  }
  const metadataFile = path.join(root, "native-plugin.json")
  const metadata = JSON.parse(await readFile(metadataFile, "utf8"))
  metadata.connectors.excalidraw.resourceTemplates.push({
    name: "document",
    uriTemplate: "ui://excalidraw/document/{id}",
  })
  await writeFile(metadataFile, JSON.stringify(metadata, null, 2) + "\n")
  const panel = path.join(root, "source/dist/ui/panel.html")
  await writeFile(
    panel,
    appendScript(
      await readFile(panel, "utf8"),
      `
// 编辑器尚未加载时也能退出；已加载时等待现有场景保存队列。
window.__pluginClient.app.onteardown = async () => {
  document.activeElement?.blur();
  await window.__excalidrawPanel?.flush();
  return {};
};`
    )
  )
  await updateScriptBytes(root)
}

async function updateScriptBytes(root) {
  const directory = path.join(root, "source/dist/ui")
  const file = path.join(directory, "assets.json")
  const index = JSON.parse(await readFile(file, "utf8"))
  const scripts = await Promise.all(
    index.scripts.map((name) => readFile(path.join(directory, name)))
  )
  index.scriptBytes = scripts.reduce((total, bytes) => total + bytes.length, 0)
  await writeFile(file, JSON.stringify(index))
}

export async function repairInfiniteCanvas(root) {
  const file = path.join(root, "widget/dist/infinite-canvas-widget.html")
  let html = await readFile(file, "utf8")
  html = replaceOnce(
    html,
    "await e.connect();let t=async(o,a)=>{",
    "let t=async(o,a)=>{"
  )
  html = replaceOnce(
    html,
    "n=qut();window.__IYW_CANVAS_BRIDGE__=",
    "n=qut();e.onteardown=async()=>(r.dispose(),n(),{});await e.connect();window.__IYW_CANVAS_BRIDGE__="
  )
  html = replaceOnce(
    html,
    "await Promise.resolve().then(()=>(dDr(),SJn)),e.onteardown=async()=>(r.dispose(),n(),{})",
    "await Promise.resolve().then(()=>(dDr(),SJn))"
  )
  html = replaceOnce(html, 'version:"0.1.22"', 'version:"0.1.23"')
  await writeFile(file, html)
  const runtime = path.join(root, "runtime/dist/infinite-canvas-mcp.mjs")
  const source = await readFile(runtime, "utf8")
  await writeFile(
    runtime,
    replaceOnce(source, 'version: "0.1.22"', 'version: "0.1.23"')
  )
}
