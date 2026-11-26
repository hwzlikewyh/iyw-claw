import { realpath, rm } from "node:fs/promises"
import { resolve, sep } from "node:path"

// 仅清理声明中当前连接器自己的授权目录，不使用 shell 删除命令。
export async function clearAuthDirectories(context) {
  const account = await realpath(context.account)
  for (const entry of context.manifest.clearAuthDirectories ?? []) {
    const target = resolve(context.expand(entry))
    if (!target.startsWith(resolve(context.account) + sep))
      throw new Error("连接器授权清理目录越界")
    let canonical
    try {
      canonical = await realpath(target)
    } catch (error) {
      if (error.code === "ENOENT") continue
      throw error
    }
    if (!canonical.startsWith(account + sep))
      throw new Error("连接器授权目录指向账号目录之外，未执行清理")
    await rm(target, { recursive: true, force: false })
  }
}
