import type { AdaptedToolCallPart } from "@/lib/adapters/ai-elements-adapter"
import { sanitizeCommandDisplayText } from "@/lib/command-display"
import { normalizeToolName } from "@/lib/tool-call-normalization"
import { getBuiltinToolDisplay } from "@/lib/builtin-tool-display"
import { commandActivity } from "./assistant-command-activity"

export type ProcessToolKind =
  | "read"
  | "write"
  | "edit"
  | "folder"
  | "search"
  | "fetch"
  | "command"
  | "skill"
  | "tool"

type Translate = (key: string, values?: Record<string, string>) => string
const MAX_ACTIVITY_LENGTH = 180
const SECRET = /Bearer\s+|(?:token|password|secret|api[_-]?key)\s*[:=]/i
const GENERIC_ACTIVITY =
  /^(?:正在\s*)?(?:执行命令|执行工作区操作|运行命令|读取文件|调用工具|查找可用工具|查看工具用法|运行\s+(?:pwsh|powershell|cmd|bash|sh)(?:\.exe)?|read file|execute command|run command)$/i
const KINDS: Record<string, ProcessToolKind> = {
  read: "read",
  read_file: "read",
  write: "write",
  write_file: "write",
  edit: "edit",
  apply_patch: "edit",
  list_directory: "folder",
  list_dir: "folder",
  list_files: "folder",
  ls: "folder",
  glob: "search",
  grep: "search",
  websearch: "search",
  webfetch: "fetch",
  skill: "skill",
  bash: "command",
  exec_command: "command",
  shell: "command",
  search_iyw_capabilities: "search",
  browser_open: "fetch",
  browser_read: "fetch",
  browser_snapshot: "fetch",
  fetch_iyw_url: "fetch",
  upload_iyw_file: "write",
}

type DisplayTool = Pick<
  AdaptedToolCallPart,
  "toolName" | "input" | "displayTitle"
>

function baseToolKind(tool: DisplayTool): ProcessToolKind {
  const name =
    getBuiltinToolDisplay(tool.toolName, tool.input)?.toolName ?? tool.toolName
  const bare =
    name
      .split(/__|[.:/]/)
      .pop()
      ?.toLowerCase() ?? ""
  return KINDS[bare] ?? KINDS[normalizeToolName(name).toLowerCase()] ?? "tool"
}

export function processToolKind(tool: DisplayTool): ProcessToolKind {
  const kind = baseToolKind(tool)
  return kind === "command"
    ? (commandActivity(processToolInput(tool.input))?.kind ?? kind)
    : kind
}

export function processToolInput(
  input: string | null
): Record<string, unknown> {
  try {
    const value: unknown = JSON.parse(input ?? "null")
    return typeof value === "string" ? { command: value } : nestedInput(value)
  } catch {
    return input && !/^[\s]*[{[]/.test(input) ? { command: input } : {}
  }
}

function nestedInput(value: unknown, depth = 0): Record<string, unknown> {
  if (depth > 4 || !value || typeof value !== "object" || Array.isArray(value))
    return {}
  const record = value as Record<string, unknown>
  return ["input", "arguments", "params", "payload"].reduce((merged, key) => {
    let nested = record[key]
    if (typeof nested === "string") {
      try {
        nested = JSON.parse(nested)
      } catch {
        return merged
      }
    }
    return { ...merged, ...nestedInput(nested, depth + 1) }
  }, record)
}

function stringField(input: Record<string, unknown>, keys: string[]) {
  for (const key of keys) {
    const value = input[key]
    if (typeof value === "string" && value.trim()) return value.trim()
  }
  return null
}

function naturalTitle(value: string | null | undefined) {
  if (!value || value.length > MAX_ACTIVITY_LENGTH || SECRET.test(value))
    return null
  if (GENERIC_ACTIVITY.test(value.trim())) return null
  if (/[\u0000-\u001f`{}[\]=|;&<>]/.test(value)) return null
  if (!/[\u3400-\u9fff]|[a-z]{2,}\s+[a-z]{2,}/i.test(value)) return null
  if (
    /^(?:git|cargo|curl|wget|node|npm|pnpm|yarn|python|powershell|pwsh|cmd|bash|sh|cat|ls|rg|Get-Content|Get-ChildItem)\b/i.test(
      value
    )
  )
    return null
  return sanitizeCommandDisplayText(value)
    .replace(/^(?:正在|准备)\s*/, "")
    .trim()
}

function targetLabel(
  tool: DisplayTool,
  input: Record<string, unknown>,
  translate: Translate
) {
  const kind = processToolKind(tool)
  const file = stringField(input, [
    "file_path",
    "filePath",
    "path",
    "filename",
    "directory",
    "dir_path",
  ])
  if (file && ["read", "write", "edit", "folder"].includes(kind)) {
    const target =
      file
        .replace(/[\\/]+$/, "")
        .split(/[\\/]/)
        .pop() || file
    return translate(kind, { target })
  }
  if (kind === "search") {
    const target = stringField(input, [
      "query",
      "pattern",
      "search_query",
      "search_term",
    ])
    const discovery = /(?:^|[._:/-])search_iyw_capabilities$/.test(
      tool.toolName
    )
    if (target && !SECRET.test(target))
      return translate(discovery ? "findTools" : "search", { target })
  }
  if (kind === "fetch") return webTargetLabel(tool, input, translate)
  if (kind === "skill") {
    const target = stringField(input, ["skill", "name"])
    if (target) return translate("skill", { target })
  }
  return null
}

function webTargetLabel(
  tool: DisplayTool,
  input: Record<string, unknown>,
  translate: Translate
) {
  try {
    const url = new URL(stringField(input, ["url", "uri"]) ?? "")
    if (url.protocol === "https:" || url.protocol === "http:") {
      const name = getBuiltinToolDisplay(tool.toolName, tool.input)?.toolName
      return translate(name === "browser_open" ? "openWeb" : "fetch", {
        target: url.hostname,
      })
    }
  } catch {
    /* 流式参数尚未形成完整 URL 时沿用原工具名。 */
  }
  return null
}

export function processToolTitle(
  tool: DisplayTool,
  translate: Translate,
  fallback: string
) {
  const input = processToolInput(tool.input)
  const command =
    baseToolKind(tool) === "command" ? commandActivity(input) : null
  if (/(?:^|[._:/-])read_iyw_capability$/.test(tool.toolName))
    return translate("toolUsage", { target: fallback })
  return (
    naturalTitle(stringField(input, ["description", "display_title"])) ??
    naturalTitle(tool.displayTitle) ??
    (command
      ? translate(command.kind === "command" ? "run" : command.kind, {
          target: command.target,
        })
      : null) ??
    targetLabel(tool, input, translate) ??
    (processToolKind(tool) === "command" ? tool.toolName : fallback)
  )
}
