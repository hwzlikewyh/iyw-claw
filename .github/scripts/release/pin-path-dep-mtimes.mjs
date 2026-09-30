// Cargo 按源码修改时间判断 path 依赖（codex 补丁等）是否需要重编。每次 checkout 都会把修改时间
// 刷新成当前时间，缓存里已编好的补丁 crate 及依赖它们的全部 crate 因此被整体重编。
// 这里把给定目录下 git 跟踪文件的修改时间固定为早于任何缓存产物的时间，并输出这些文件内容的哈希。
// 调用方必须把哈希放进缓存的恢复 key，保证恢复出的产物与当前源码内容一致，否则会复用旧产物。
import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import { appendFileSync, readFileSync, utimesSync } from "node:fs"

const roots = process.argv.slice(2)
if (roots.length === 0) {
  throw new Error("usage: pin-path-dep-mtimes.mjs <dir>...")
}
const files = execFileSync("git", ["ls-files", "-z", "--", ...roots], {
  encoding: "utf8",
})
  .split("\0")
  .filter(Boolean)
  .sort()
if (files.length === 0) {
  throw new Error(`no tracked files under ${roots.join(", ")}`)
}

const pinned = new Date("2000-01-01T00:00:00Z")
const hash = createHash("sha256")
for (const file of files) {
  hash.update(file).update("\0").update(readFileSync(file)).update("\0")
  utimesSync(file, pinned, pinned)
}
const digest = hash.digest("hex").slice(0, 16)
console.log(
  `[path-deps] pinned ${files.length} files under ${roots.join(", ")}; content ${digest}`
)
if (process.env.GITHUB_OUTPUT) {
  appendFileSync(process.env.GITHUB_OUTPUT, `hash=${digest}\n`)
}
