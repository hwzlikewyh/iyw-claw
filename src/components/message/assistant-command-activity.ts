import { commandText, commandTokens } from "./assistant-command-input"

type CommandActivity = {
  kind: "read" | "folder" | "search" | "command"
  target: string
}

const READ_COMMANDS = new Set(["get-content", "gc", "cat", "type"])
const FOLDER_COMMANDS = new Set(["get-childitem", "gci", "ls", "dir"])
const SEARCH_COMMANDS = new Set(["rg", "grep", "select-string"])
const CONTEXT_COMMANDS = new Set(["pwd", "get-location", "cd", "set-location"])
const DISPLAY_SUBCOMMANDS = new Set([
  "build",
  "check",
  "test",
  "lint",
  "status",
  "diff",
  "install",
  "run",
  "dev",
  "exec",
])
const VALUE_OPTIONS = new Set([
  "-totalcount",
  "-tail",
  "-encoding",
  "-filter",
  "-include",
  "-exclude",
  "-g",
  "--glob",
  "-t",
  "--type",
  "--type-not",
  "-m",
  "--max-count",
])

function unquote(value: string) {
  return value.replace(/^(["'])(.*)\1$/, "$2")
}

function targetArgument(tokens: string[]) {
  const explicit = tokens.findIndex((token) =>
    /^-(?:literalpath|path|pattern|e|regexp)$/i.test(token)
  )
  if (explicit >= 0) return tokens[explicit + 1] ?? null
  for (let index = 0; index < tokens.length; index += 1) {
    if (VALUE_OPTIONS.has(tokens[index].toLowerCase())) {
      index += 1
      continue
    }
    if (!tokens[index].startsWith("-")) return tokens[index]
  }
  return null
}

function safeTarget(value: string | null, leaf: boolean) {
  if (!value) return null
  const text = unquote(value).replace(/[\\/]+$/, "")
  if (!text || /[\r\n$`{}|;&<>]/.test(text)) return null
  return leaf ? (text.split(/[\\/]/).pop() ?? null) : text
}

function readOperation(
  tokens: string[],
  input: Record<string, unknown>
): CommandActivity | null {
  const executable = unquote(tokens[0]).toLowerCase()
  const target = targetArgument(tokens.slice(1))
  if (READ_COMMANDS.has(executable)) {
    const file = safeTarget(target, true)
    return file ? { kind: "read", target: file } : null
  }
  if (FOLDER_COMMANDS.has(executable)) {
    const directory = target ?? input.workdir ?? input.cwd
    const name = safeTarget(
      typeof directory === "string" ? directory : null,
      true
    )
    return name ? { kind: "folder", target: name } : null
  }
  if (SEARCH_COMMANDS.has(executable) && !tokens.includes("--files")) {
    const query = safeTarget(target, false)
    return query ? { kind: "search", target: query } : null
  }
  return null
}

export function commandActivity(
  input: Record<string, unknown>
): CommandActivity | null {
  const raw = commandText(input)
  if (!raw) return null
  // 只识别明确的只读动作，不执行命令，也不推测变量展开后的值。
  const commands = raw.split(/;|\r?\n|&&|\|\|/)
  for (const command of commands) {
    const tokens = commandTokens(command.trim())
    if (!tokens.length) continue
    const operation = readOperation(tokens, input)
    if (operation) return operation
    if (!CONTEXT_COMMANDS.has(unquote(tokens[0]).toLowerCase())) break
  }
  const tokens = raw.trim().match(/"[^"\r\n]*"|'[^'\r\n]*'|[^\s;|]+/g)
  const executable = safeTarget(tokens?.[0] ?? null, true)
  if (!executable || !/^[\p{L}\p{N}_.-]+$/u.test(executable)) return null
  const subcommand = tokens?.[1]
  const target =
    subcommand && DISPLAY_SUBCOMMANDS.has(subcommand)
      ? executable + " " + subcommand
      : executable
  return { kind: "command", target }
}
