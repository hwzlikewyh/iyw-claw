import { execFileSync } from "node:child_process"
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { environmentHelperBuildOptions } from "./environment-helper-runtime.mjs"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const NAME = "iyw-computer-helper"

export function prepareComputerHelper(target, debug = false) {
  const options = environmentHelperBuildOptions(target)
  const extension = target.includes("windows") ? ".exe" : ""
  execFileSync(
    "cargo",
    [
      "build",
      ...(debug ? [] : ["--release"]),
      "--manifest-path",
      join(ROOT, "Cargo.toml"),
      "--no-default-features",
      "--features",
      "computer-helper",
      "--bin",
      NAME,
      "--target",
      target,
      ...options.args,
    ],
    { cwd: ROOT, stdio: "inherit", env: options.env }
  )
  const executable = join(
    ROOT,
    "target",
    target,
    debug ? "debug" : "release",
    NAME + extension
  )
  if (!existsSync(executable) || statSync(executable).size === 0)
    throw new Error("Computer helper was not built")
  const binaries = join(ROOT, "binaries")
  mkdirSync(binaries, { recursive: true })
  const staged = join(binaries, `${NAME}-${target}${extension}`)
  copyFileSync(executable, staged)
  if (target.endsWith("apple-darwin")) prepareMacBundle(executable)
  return staged
}

function prepareMacBundle(executable) {
  const app = join(ROOT, "macos", `${NAME}.app`)
  const contents = join(app, "Contents")
  mkdirSync(join(contents, "MacOS"), { recursive: true })
  copyFileSync(executable, join(contents, "MacOS", NAME))
  const version = JSON.parse(
    readFileSync(join(ROOT, "..", "package.json"), "utf8")
  ).version
  const plist = readFileSync(
    join(ROOT, "macos", `${NAME}.plist`),
    "utf8"
  ).replaceAll("{{version}}", version)
  writeFileSync(join(contents, "Info.plist"), plist)
  const identity = process.env.APPLE_SIGNING_IDENTITY || "-"
  execFileSync(
    "codesign",
    [
      "--force",
      "--options",
      "runtime",
      ...(identity === "-" ? [] : ["--timestamp"]),
      "--sign",
      identity,
      app,
    ],
    { stdio: "inherit" }
  )
  execFileSync("codesign", ["--verify", "--strict", app], { stdio: "inherit" })
}
