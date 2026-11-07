import { cp, readFile, readdir, writeFile, mkdir } from "node:fs/promises"
import { createHash } from "node:crypto"
import path from "node:path"
import { patchSpreadsheet } from "./spreadsheet.mjs"
import { adaptPanel } from "./panel.mjs"

const PORT_VERSION = "0.1.0"
const hash = (value) => createHash("sha256").update(value).digest("hex")
const json = (value) => `${JSON.stringify(value, null, 2)}\n`

export async function prepareFork({ source, fork, name, config }) {
  const upstream = JSON.parse(
    await readFile(path.join(source, ".zcode-plugin/plugin.json"), "utf8")
  )
  if (upstream.name !== name || upstream.version !== config.version)
    throw Error("Upstream plugin identity changed")
  await mkdir(fork)
  await cp(path.join(source, "dist"), path.join(fork, "dist"), {
    recursive: true,
    errorOnExist: true,
    force: false,
  })
  await cp(path.join(source, "skills"), path.join(fork, "skills"), {
    recursive: true,
    errorOnExist: true,
    force: false,
  })
  const serverPath = path.join(fork, "dist/server.mjs")
  const original = await readFile(serverPath, "utf8")
  const patched = patchServer(original, name)
  await writeFile(serverPath, patched)
  await adaptPanel(fork)
  await adaptSkills({ source, fork, name })
  await writeFile(
    path.join(fork, "entry.mjs"),
    `import { main } from "./dist/server.mjs"\nimport { createHash } from "node:crypto"\nimport { realpath } from "node:fs/promises"\nif (!process.env.IYW_WORKSPACE_DIR || !process.env.IYW_PLUGIN_DATA_DIR) throw Error("Original assistant scoped workspace is required")\nprocess.env.IYW_OFFICE_SCOPE = createHash("sha256").update(await realpath(process.env.IYW_PLUGIN_DATA_DIR)).digest("hex")\nprocess.chdir(process.env.IYW_WORKSPACE_DIR)\nawait main()\n`
  )
  await writeMetadata({ fork, name, config, upstream, original })
}

async function writeMetadata({ fork, name, config, upstream, original }) {
  await mkdir(path.join(fork, ".codex-plugin"))
  await writeFile(
    path.join(fork, ".codex-plugin/plugin.json"),
    json({
      name: `iyw-${name}`,
      version: PORT_VERSION,
      description: `${config.title}制作、阅读与审阅`,
      skills: "skills",
      mcpServers: ".mcp.json",
      license: "MIT AND Apache-2.0",
    })
  )
  await writeFile(
    path.join(fork, ".mcp.json"),
    json({
      mcpServers: {
        [name]: { command: "node", args: ["entry.mjs"], cwd: ".", env: {} },
      },
    })
  )
  await writeProvenance({ fork, name, config, upstream, original })
  await writeReadme({ fork, name, config })
}

async function writeProvenance({ fork, name, config, upstream, original }) {
  await writeFile(
    path.join(fork, "port-provenance.json"),
    json({
      schemaVersion: 1,
      name: `iyw-${name}`,
      version: PORT_VERSION,
      upstream: {
        name,
        version: config.version,
        author: upstream.author,
        license: upstream.license,
        serverSha256: hash(original),
      },
      changes: [
        "managed-node-main",
        "iyw-workspace",
        "account-scoped-imports",
        "conversation-scoped-state",
        "iyw-panel-layout",
        ...(name === "documents" ? ["docx-initial-fit"] : []),
        ...(name === "spreadsheets" ? ["xlsx-namespace-prefixes"] : []),
        ...(["spreadsheets", "pdf"].includes(name)
          ? ["opaque-origin-module-worker"]
          : []),
        "scoped-resources-and-wasm",
        "host-skill-routing",
        "visual-review-reference",
      ],
    })
  )
}

async function writeReadme({ fork, name, config }) {
  await writeFile(
    path.join(fork, "README.md"),
    `# ${config.title}\n\n原助理独立插件包，用户自行从云端选择安装并授权当前项目，不自动安装或启用。\n\n改编自Z.ai/ZCode ${name} ${config.version}，保留dist/LICENSE（MIT）、THIRD_PARTY_NOTICES和技能目录的Apache-2.0许可。修改记录见port-provenance.json。\n\n运行使用宿主管理的Node与当前会话工作区；远程模式读取服务器工作区。文档制作脚本所需依赖沿宿主受管环境准备，不从插件自行安装运行时。面板为只读预览，制作/编辑使用技能与原有文件流程。\n\n此制品仍须通过真实导入/预览/引用/导出和发布验收，不以目录存在表示已发布或已安装。\n`
  )
}

function patchServer(source, name) {
  const session = /function sessionOf\(meta3\) \{[\s\S]*?\n\}/g
  if ([...source.matchAll(session)].length !== 1)
    throw Error("Upstream conversation routing changed")
  source = source.replace(
    session,
    `function sessionOf(meta3) { const id = meta3?.["iyw/conversationId"]; if (typeof id !== "string" || !/^[1-9][0-9]{0,9}$/.test(id)) throw new OfficeError("invalid_token", "Trusted conversation context is required."); return id; }`
  )
  const changes = [
    [
      "{ mimeType: HTML_MIME, _meta: panelMeta },",
      '{ title: PROFILE.title["zh-CN"], mimeType: HTML_MIME, _meta: panelMeta },',
    ],
    ["var SERVER_NAME = PROFILE.name;", `var SERVER_NAME = "iyw-${name}";`],
    [
      "var PLUGIN_VERSION = PROFILE.version;",
      `var PLUGIN_VERSION = "${PORT_VERSION}";`,
    ],
    [
      'const directory = join(base, ".zcode", "document-imports");',
      `const directory = join(base, ".iyw", "plugin-imports", "iyw-${name}", process.env.IYW_OFFICE_SCOPE);`,
    ],
    [
      "const workspace = process.env.ZCODE_WORKSPACE_ROOT || process.cwd();",
      "const workspace = process.env.IYW_WORKSPACE_DIR || process.cwd();",
    ],
    [
      'const panelMeta = { "zcode/csp": { wasmUnsafeEval: true } };',
      'const panelMeta = { ui: { csp: { wasmUnsafeEval: true, resourceDomains: ["blob:", "data:"] } } };',
    ],
  ]
  for (const [before, after] of changes) {
    if (source.split(before).length !== 2)
      throw Error("Upstream office runtime changed; review the port transform")
    source = source.replace(before, after)
  }
  return name === "spreadsheets" ? patchSpreadsheet(source) : source
}

async function adaptSkills({ source, fork, name }) {
  const judge = await readFile(
    path.join(source, "agents/visual-judge.md"),
    "utf8"
  ).catch((cause) => {
    if (cause.code === "ENOENT") return null
    throw cause
  })
  for (const entry of await readdir(path.join(fork, "skills"), {
    withFileTypes: true,
  })) {
    if (!entry.isDirectory()) continue
    const directory = path.join(fork, "skills", entry.name)
    const file = path.join(directory, "SKILL.md")
    let text = await readFile(file, "utf8")
    text = text.replace(
      /^name: .*$/m,
      `name: iyw-${name}${entry.name.endsWith("-viewer") ? "-viewer" : ""}`
    )
    text = text.replace(/\n## Final response citations[\s\S]*$/, "")
    text = text.replace(
      /The panel only works in ZCode Desktop[^\n]*/,
      "面板使用原助理插件宿主，读取当前会话所在主机的工作区；远程会话读取服务器工作区。"
    )
    text = text.replace(
      /use the visual-judge subagent \(if not present, check it yourself\)/g,
      "按references/visual-review.md由当前代理自行检查"
    )
    text = text.replace(
      /^.*`capture_view`.*$/gm,
      "需要查看排版时，使用已托管文档渲染能力生成PNG并检查，不调用尚未注册的面板专属工具。"
    )
    if (
      text.includes("license: Proprietary") &&
      (await hasApacheLicense(directory))
    )
      text = text.replace("license: Proprietary", "license: Apache-2.0")
    text += `\n\n## 原助理宿主约定\n\n通过search_iyw_capabilities查找本插件工具，再用invoke_iyw_capability按capability_id调用；正文中的工具名为功能说明，不直接调用旧宿主工具。只使用模型可见工具。文件引用使用本宿主Markdown文件链接并说明页码/单元格，不输出ZCode专属引用语法。环境依赖沿原助理受管环境准备，不自动运行pip/npm安装运行时。视觉验收由当前代理按references/visual-review.md自行检查，不创建子代理；面板不能编辑文件，不把预览说成保存成功。\n`
    await writeFile(file, text)
    if (judge) {
      await mkdir(path.join(directory, "references"), { recursive: true })
      await writeFile(
        path.join(directory, "references/visual-review.md"),
        `# 视觉验收\n\n本说明由原visual-judge转换为当前代理的复核参考；不创建子代理，也不选择旧宿主模型。以下“分派”仅指当前任务指定的页码和需求。只检查已经渲染的PNG，缺少证据明确记录未验证。\n\n${judge.replace(/^---[\s\S]*?---\s*/, "")}`
      )
    }
  }
}

async function hasApacheLicense(directory) {
  const license = await readFile(
    path.join(directory, "LICENSE.txt"),
    "utf8"
  ).catch((cause) => {
    if (cause.code === "ENOENT") return ""
    throw cause
  })
  return license.includes("Apache License")
}
