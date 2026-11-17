import { execFileSync } from "node:child_process"
import { existsSync, mkdtempSync, readFileSync, readdirSync, renameSync, writeFileSync } from "node:fs"
import { join } from "node:path"

export function readDebControl(path) {
  const archive = execFileSync("dpkg-deb", ["--ctrl-tarfile", path])
  const entries = execFileSync("tar", ["-tf", "-"], { input: archive, encoding: "utf8" })
    .trim().split("\n").filter(name => name === "control" || name === "./control")
  if (entries.length !== 1) throw new Error("DEB control entry is ambiguous")
  return execFileSync("tar", ["-xOf", "-", entries[0]], { input: archive, encoding: "utf8" })
}

// Tauri 将中文 productName 写入 Package；只修控制字段，保留已编译程序依赖的资源路径。
export function fixDebControl(target) {
  const root = join("src-tauri/target", target, "release/bundle/deb")
  const files = readdirSync(root).filter(name => name.endsWith(".deb"))
  if (files.length !== 1) throw new Error("expected one DEB to validate")
  const path = join(root, files[0])
  const control = readDebControl(path)
  const current = control.match(/^Package:\s*(.+)$/m)?.[1].trim()
  if (current && /^[a-z0-9][a-z0-9+.-]+$/.test(current)) return
  const name = JSON.parse(readFileSync("package.json", "utf8")).name
  if (!/^[a-z0-9][a-z0-9+.-]+$/.test(name)) throw new Error("invalid canonical DEB package name")
  const staging = mkdtempSync(join(process.env.RUNNER_TEMP, "valid-deb-"))
  execFileSync("dpkg-deb", ["--extract", path, staging], { stdio: "inherit" })
  const metadata = join(staging, "DEBIAN")
  execFileSync("dpkg-deb", ["--control", path, metadata], { stdio: "inherit" })
  writeFileSync(join(metadata, "control"), control.replace(/^Package:.*$/m, `Package: ${name}`))
  const rebuilt = `${path}.valid`
  execFileSync("dpkg-deb", ["--build", "--root-owner-group", "-Zgzip", "-z1", staging, rebuilt], { stdio: "inherit" })
  renameSync(rebuilt, path)
  if (execFileSync("dpkg-deb", ["--field", path, "Package"], { encoding: "utf8" }).trim() !== name)
    throw new Error("canonical DEB control validation failed")
  if (existsSync(`${path}.sig`))
    execFileSync(process.execPath, ["node_modules/@tauri-apps/cli/tauri.js", "signer", "sign", path], { stdio: "inherit" })
  console.log(`[linux-deb] validated Package=${name}; application and resource paths preserved`)
}
