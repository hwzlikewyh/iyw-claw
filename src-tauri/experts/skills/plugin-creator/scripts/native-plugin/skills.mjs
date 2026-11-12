import { cp, readFile, readdir, writeFile } from "node:fs/promises"
import path from "node:path"
const KEY = /^[a-z0-9]+(?:-[a-z0-9]+)*$/

export async function addSkills(state, source) {
  const relative = source.manifest.skills ?? "./skills/"
  if (typeof relative !== "string" || path.isAbsolute(relative))
    throw Error("Native skills must reference a relative directory")
  const directory = path.resolve(source.root, relative)
  if (path.relative(source.root, directory).startsWith(".."))
    throw Error("Native skills escape plugin root")
  let entries
  try {
    entries = await readdir(directory, { withFileTypes: true })
  } catch (error) {
    if (error.code === "ENOENT" && !source.manifest.skills) return
    throw error
  }
  for (const entry of entries.filter((item) => item.isDirectory())) {
    if (!KEY.test(entry.name)) throw Error(`Invalid skill key: ${entry.name}`)
    const skillPath = path.join(directory, entry.name, "SKILL.md")
    let text
    try {
      text = await readFile(skillPath, "utf8")
    } catch (error) {
      if (error.code === "ENOENT") continue
      throw error
    }
    const target = path.join(state.output, "skills", entry.name)
    await cp(path.dirname(skillPath), target, {
      recursive: true,
      force: false,
      errorOnExist: true,
    })
    const routes = state.components.capabilities.filter((tool) =>
      state.native.connectors[tool.connectorKey].modelTools.includes(
        tool.toolName
      )
    )
    text += `\n\n## 原助理接入\n\n通过 search_iyw_capabilities 查找插件能力，再以 invoke_iyw_capability 调用对应 capability_id。不得把应用专属工具作为 Agent 工具调用。工具映射：\n\n${routes.map((tool) => `- ${tool.toolName}: ${tool.id}`).join("\n")}\n`
    await writeFile(path.join(target, "SKILL.md"), text)
    state.components.skills.push({
      key: entry.name,
      path: `skills/${entry.name}`,
      requiresConnectors: Object.keys(source.servers),
    })
  }
}
