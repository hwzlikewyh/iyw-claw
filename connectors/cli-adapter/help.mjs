import { readFile } from "node:fs/promises"
import { resolve, sep } from "node:path"

// 部分供应商禁用程序内帮助，保留其随连接器发布的命令参考。
export async function bundledHelp(context, tool) {
  const sections = [
    "以下为供应商命令参考。调用本连接器时将 CLI 参数放入 args 数组，省略程序名。",
  ]
  for (const name of tool.helpFiles) {
    const path = resolve(context.artifact, name)
    if (!path.startsWith(resolve(context.artifact) + sep))
      throw new Error("命令参考路径越界")
    sections.push(await readFile(path, "utf8"))
  }
  return { content: [{ type: "text", text: sections.join("\n\n") }] }
}
