import { copyFile, mkdir, readFile } from "node:fs/promises"
import { createHash } from "node:crypto"
import { dirname, resolve, sep } from "node:path"

// 自解压和自更新程序必须在账号运行目录执行，不能修改共享制品缓存。
export async function prepareNativeFiles(context) {
  for (const entry of context.manifest.runtimeFiles ?? []) {
    const source = resolve(context.artifact, entry.path)
    const target = resolve(context.runtime, entry.path)
    if (
      !source.startsWith(resolve(context.artifact) + sep) ||
      !target.startsWith(resolve(context.runtime) + sep)
    )
      throw new Error("连接器运行文件路径越界")
    const digest = createHash("sha256")
      .update(await readFile(source))
      .digest("hex")
    if (digest !== entry.sha256)
      throw new Error("连接器程序摘要不匹配，请重新安装制品")
    await mkdir(dirname(target), { recursive: true })
    await copyFile(source, target)
  }
}
