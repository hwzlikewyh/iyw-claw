import { createReadStream } from "node:fs"
import { lstat, readdir, readFile } from "node:fs/promises"
import { createHash } from "node:crypto"
import { resolve, join } from "node:path"

const MAX_FILES = 512

async function digest(path) {
  const hash = createHash("sha256")
  for await (const bytes of createReadStream(path)) hash.update(bytes)
  return hash.digest("hex")
}

async function manifest(directory, prefix = "", files = []) {
  const entries = await readdir(directory, { withFileTypes: true })
  for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name))) {
    const path = join(directory, entry.name)
    const relative = prefix + entry.name
    if (entry.isSymbolicLink()) throw Error(`不支持符号链接：${relative}`)
    if (entry.isDirectory()) {
      await manifest(path, relative + "/", files)
    } else if (entry.isFile()) {
      if (files.length >= MAX_FILES) throw Error("包文件数量超过 512")
      const stat = await lstat(path)
      if (!Number.isSafeInteger(stat.size)) throw Error("文件大小超出精确整数范围")
      files.push({
        path: relative,
        size: stat.size,
        sha256: await digest(path),
        mimeType: "application/octet-stream",
      })
    } else throw Error(`不支持的文件类型：${relative}`)
  }
  return files
}

async function put(file, targetPath) {
  const target = JSON.parse(await readFile(targetPath, "utf8"))
  const url = new URL(target.url)
  if (url.protocol !== "https:" || url.username || url.password || url.hash || target.method !== "PUT")
    throw Error("需要当前上传会话签发的 HTTPS PUT 地址")
  const stat = await lstat(file)
  if (!stat.isFile() || !Number.isSafeInteger(stat.size)) throw Error("需要真实普通文件")
  if (target.size != null && target.size !== stat.size) throw Error("文件大小已变化，请重新初始化")
  if (target.sha256 && target.sha256 !== await digest(file)) throw Error("文件摘要已变化，请重新初始化")
  const headers = new Headers()
  for (const [key, value] of Object.entries(target.headers || {})) {
    if (!/^(host|content-length|token|tokeninfo|admin-token|authorization|cookie)$/i.test(key))
      headers.set(key, value)
  }
  headers.set("Content-Length", String(stat.size))
  const response = await fetch(url, {
    method: "PUT", headers, body: createReadStream(file), duplex: "half",
    redirect: "error", credentials: "omit", signal: AbortSignal.timeout(10 * 60_000),
  })
  await response.body?.cancel()
  if (!response.ok) throw Error(`TOS 上传未完成（HTTP ${response.status}），先查询原任务`)
  process.stdout.write(JSON.stringify({ uploaded: true, size: stat.size }) + "\n")
}

const [action, path, targetPath] = process.argv.slice(2)
try {
  if (action === "manifest" && path) {
    const files = await manifest(resolve(path))
    if (!files.length) throw Error("目录不能为空")
    process.stdout.write(JSON.stringify({ files }, null, 2) + "\n")
  } else if (action === "put" && path && targetPath) {
    await put(resolve(path), resolve(targetPath))
  } else {
    process.stdout.write("Node.js 22+: market-direct-upload.mjs manifest DIRECTORY | put FILE TARGET_JSON\n")
    if (action && action !== "--help") process.exitCode = 1
  }
} catch (error) {
  // 不输出预签名 URL、请求头或底层网络错误中的凭据。
  process.stderr.write((error instanceof TypeError ? "上传输入或网络请求失败" : error.message) + "\n")
  process.exitCode = 1
}
