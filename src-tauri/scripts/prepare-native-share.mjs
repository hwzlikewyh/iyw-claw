import { execFileSync } from "node:child_process"
import { copyFileSync, mkdirSync, readdirSync, rmSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { prepareWindowsShare } from "./prepare-native-share-windows.mjs"
import { prepareMacShare } from "./prepare-native-share-macos.mjs"

export const SHARE_ROOT = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../native-share"
)

export function prepareNativeShare(target) {
  mkdirSync(join(SHARE_ROOT, "resources"), { recursive: true })
  // 这里只保存本脚本生成的制品，切换平台或签名模式时不能携带旧包。
  for (const name of readdirSync(join(SHARE_ROOT, "resources"))) {
    if (name !== ".gitkeep")
      rmSync(join(SHARE_ROOT, "resources", name), {
        recursive: true,
        force: true,
      })
  }
  copyFileSync(
    join(SHARE_ROOT, "UPSTREAM-LICENSE"),
    join(SHARE_ROOT, "resources/UPSTREAM-LICENSE")
  )
  if (target.endsWith("-windows-msvc"))
    copyFileSync(
      join(SHARE_ROOT, "windows/register.ps1"),
      join(SHARE_ROOT, "resources/register.ps1")
    )
  if (target.includes("win7") || target.includes("linux")) return
  if (target.endsWith("-windows-msvc")) prepareWindowsShare(target, SHARE_ROOT)
  else if (target.endsWith("-apple-darwin")) prepareMacShare(target, SHARE_ROOT)
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const index = process.argv.indexOf("--target")
  const target =
    index >= 0
      ? process.argv[index + 1]
      : process.env.TAURI_TARGET_TRIPLE ||
        execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(
          /^host: (.+)$/m
        )?.[1]
  if (!target) throw new Error("Cannot determine the native share build target")
  prepareNativeShare(target)
}
