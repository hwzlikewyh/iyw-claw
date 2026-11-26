import { mkdir, readFile, rename, unlink, writeFile } from "node:fs/promises"
import { dirname, resolve, sep } from "node:path"

export async function applyDefaults(context) {
  for (const entry of context.manifest.configDefaults ?? []) {
    const target = resolve(context.expand(entry.path))
    if (!target.startsWith(resolve(context.account) + sep)) {
      throw new Error("连接器默认配置超出账号目录")
    }
    let existing = {}
    try {
      existing = JSON.parse(await readFile(target, "utf8"))
    } catch (error) {
      if (error.code !== "ENOENT") throw error
    }
    if (!existing || Array.isArray(existing) || typeof existing !== "object") {
      throw new Error("连接器现有配置格式无效，未覆盖原文件")
    }
    await mkdir(dirname(target), { recursive: true })
    const temporary = `${target}.${process.pid}.tmp`
    try {
      await writeFile(
        temporary,
        JSON.stringify({ ...entry.values, ...existing }, null, 2),
        { flag: "wx" }
      )
      await rename(temporary, target)
    } finally {
      await unlink(temporary).catch((error) => {
        if (error.code !== "ENOENT") throw error
      })
    }
  }
}
