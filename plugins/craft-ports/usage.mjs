import { mkdir, writeFile } from "node:fs/promises"
import path from "node:path"
const VERSION = "0.1.0"
const json = (value) => `${JSON.stringify(value, null, 2)}\n`

export async function writeUsageFiles(output, provenance, config) {
  await mkdir(path.join(output, "skills", config.skill), { recursive: true })
  await writeFile(
    path.join(output, "skills", config.skill, "SKILL.md"),
    `---\nname: ${config.name}\ndescription: 在当前项目内使用 ${config.title} 打开、编辑和导出${config.photo ? "图像" : "PDF"}，与用户共用插件面板。\n---\n\n# ${config.title}\n\n通过search_iyw_capabilities发现插件能力，再用invoke_iyw_capability按capability_id调用。open_editor仅打开页面，不代表已经编辑。可选path必须是当前项目相对路径。页面就绪后才调用编辑工具；先读取文档状态，逐项核对结果，超时后查询实际状态，不自动重试写入。\n\n用户手动导出使用原助理保存确认，保存到当前设备。模型工具写项目文件前需要当前项目写入授权，已有路径保留原覆盖行为并须用户明确要求。不要把打开、发出下载或得到文件名当作落盘成功。\n\n此插件基于${config.title} 0.2.4修改，非上游官方产品；原许可及第三方声明在source/dist/licenses目录，修改记录见source/port-provenance.json。\n`
  )
  await writeFile(
    path.join(output, "port-provenance.json"),
    json({
      name: config.name,
      version: VERSION,
      source: `${config.source}@0.2.4`,
      ...provenance,
      changes: [
        "iyw-workspace-and-name",
        ...(config.photo ? ["static-data-brand-text"] : []),
        "conversation-editor-bridge",
        "brotli-asset-integrity",
        "host-confirmed-export",
        "managed-private-editor-storage",
        ...(config.photo ? ["restore-interface-after-upstream-bootstrap"] : []),
      ],
    })
  )
  await writeFile(
    path.join(output, "README.md"),
    `# ${config.title} 原助理适配版\n\n基于${config.title} 0.2.4修改的独立插件，非上游官方构建。MIT/Apache-2.0及第三方许可证完整保留在dist/licenses。${config.photo ? "PhotoCraft WASM仅修改静态数据段的等长品牌文字并验证模块。" : "PdfCraft WASM保持源文件逐字节不变。"}Brotli压缩不删除功能或许可，解压后仍按实际摘要校验。\n\n用户从云端自主选择安装和授权，不预置。手动导出需原助理宿主保存能力；最低客户端版本声明于插件清单。\n`
  )
}
