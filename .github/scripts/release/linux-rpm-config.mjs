import { readFileSync } from "node:fs"
import { dirname, join, posix, resolve } from "node:path"
import { fileURLToPath } from "node:url"

export function rpmBundleConfig() {
  const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"))
  const name = JSON.parse(readFileSync("package.json", "utf8")).name
  if (!/^[a-z0-9][a-z0-9+.-]+$/.test(name)) throw new Error("invalid canonical RPM package name")
  const files = { ...config.bundle.linux?.rpm?.files }
  // package name 必须为 ASCII；已编译应用仍按原 productName 查找 /usr/lib 下的资源。
  for (const [source, destination] of Object.entries(config.bundle.resources))
    files[posix.join("/usr/lib", config.productName, destination)] = resolve("src-tauri", source)
  const template = join(dirname(fileURLToPath(import.meta.url)), "linux-rpm.desktop.hbs")
  return { productName: name, bundle: { resources: null, linux: { rpm: {
    compression: { type: "zstd", level: 1 }, desktopTemplate: template, files,
  } } } }
}
