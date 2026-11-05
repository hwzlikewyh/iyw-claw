import { mkdir, mkdtemp, readdir, realpath, rename, rm } from "node:fs/promises"
import path from "node:path"
import { buildManifest } from "./native-plugin/manifest.mjs"
import { copyNativeSource, readNativeSource } from "./native-plugin/source.mjs"
import { packageStats } from "./native-plugin/package.mjs"

function options(args) {
  if (args.includes("--help")) return null
  const result = {}
  for (let index = 0; index < args.length; index += 2) {
    const key = args[index]
    if (
      !["--source", "--output"].includes(key) ||
      !args[index + 1] ||
      result[key]
    )
      throw Error("Use --source <plugin-directory> --output <parent-directory>")
    result[key] = args[index + 1]
  }
  if (!result["--source"] || !result["--output"])
    throw Error("Use --source <plugin-directory> --output <parent-directory>")
  return result
}

async function main() {
  const args = options(process.argv.slice(2))
  if (!args) {
    console.log(
      "node scripts/import-native-plugin.mjs --source <native-plugin-directory> --output <parent-directory>"
    )
    return
  }
  const source = await readNativeSource(path.resolve(args["--source"]))
  const output = path.join(path.resolve(args["--output"]), source.manifest.name)
  const within = path.relative(source.root, output)
  if (!within.startsWith("..") && !path.isAbsolute(within))
    throw Error("Output must be outside the source plugin")
  try {
    await readdir(output)
    throw Error("Output already exists; choose a new directory")
  } catch (error) {
    if (error.code !== "ENOENT") throw error
  }
  await mkdir(path.dirname(output), { recursive: true })
  const staging = await mkdtemp(
    path.join(path.dirname(output), ".native-import-")
  )
  await convertSource(source, output, staging)
}

async function convertSource(source, output, staging) {
  try {
    await copyNativeSource(source.root, path.join(staging, "source"), [
      source.configPath,
    ])
    const result = await buildManifest(
      { ...source, root: await realpath(path.join(staging, "source")) },
      staging
    )
    const stats = await packageStats(staging)
    await rename(staging, output)
    console.log(
      JSON.stringify(
        {
          ...result,
          ...stats,
          output,
          nextStep:
            "在原助理插件市场的上传入口选择此目录，然后安装并授权插件。",
        },
        null,
        2
      )
    )
  } finally {
    // 仅清理由本次 mkdtemp 创建的暂存目录；正式输出和已有目录不在清理范围。
    if (
      path.dirname(staging) === path.dirname(output) &&
      path.basename(staging).startsWith(".native-import-")
    )
      await rm(staging, { recursive: true, force: true })
  }
}

main().catch((error) => {
  console.error(`[native-plugin-import] ${error.message}`)
  process.exitCode = 1
})
