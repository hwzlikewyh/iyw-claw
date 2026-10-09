import { writeFileSync } from "node:fs"
import { win7WebviewConfig } from "./build-desktop-windows.mjs"

const args = process.argv.slice(2)
if (args.length && (args.length !== 2 || args[0] !== "--config" || !args[1]))
  throw new Error("Expected --config <output path>")
const target = process.env.TAURI_TARGET_TRIPLE
if (!target) throw new Error("TAURI_TARGET_TRIPLE is required")
const config = win7WebviewConfig(target)
if (!config) throw new Error(`Unsupported Win7 target: ${target}`)
if (args.length) writeFileSync(args[1], JSON.stringify(config) + "\n")
console.log(`[win7-webview] staged fixed runtime for ${target}`)
