import { readFile, realpath } from "node:fs/promises"
import path from "node:path"
import { fileURLToPath, pathToFileURL } from "node:url"

export function nativeEnvironment(values, root) {
  return Object.fromEntries(
    Object.entries(values ?? {}).map(([key, value]) => {
      if (typeof value !== "string" || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key))
        throw Error("Invalid native MCP environment")
      const resolved = value.replace(
        /\$\{([A-Za-z_][A-Za-z0-9_]*)(?::-([^}]*))?\}/g,
        (_, name, fallback) => {
          if (["CLAUDE_PLUGIN_ROOT", "CODEX_PLUGIN_ROOT"].includes(name))
            return root
          const inherited = process.env[name] ?? fallback
          if (inherited === undefined)
            throw Error(`Missing native MCP environment variable: ${name}`)
          return inherited
        }
      )
      return [key, resolved]
    })
  )
}

// 导入器为每个 connector 生成 settings；源包保留在独立目录中。
export async function startNativeRuntime(settingsPath) {
  settingsPath =
    settingsPath instanceof URL ? fileURLToPath(settingsPath) : settingsPath
  const settings = JSON.parse(await readFile(settingsPath, "utf8"))
  const root = await realpath(
    path.resolve(path.dirname(settingsPath), settings.sourceRoot)
  )
  const entrypoint = await realpath(path.resolve(root, settings.entrypoint))
  const relative = path.relative(root, entrypoint)
  if (relative.startsWith("..") || path.isAbsolute(relative))
    throw Error("Native entrypoint escapes plugin root")
  for (const [key, value] of Object.entries(
    nativeEnvironment(settings.env, root)
  )) {
    process.env[key] = value
  }
  process.env.CLAUDE_PLUGIN_ROOT = root
  process.env.CODEX_PLUGIN_ROOT = root
  const cwd = settings.cwd ?? "."
  const workingDir = await realpath(path.resolve(root, cwd))
  const cwdRelative = path.relative(root, workingDir)
  if (cwdRelative.startsWith("..") || path.isAbsolute(cwdRelative))
    throw Error("Native working directory escapes plugin root")
  process.chdir(workingDir)
  await import(pathToFileURL(entrypoint).href)
}
