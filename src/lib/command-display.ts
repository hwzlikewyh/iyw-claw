import { normalizeToolName } from "@/lib/tool-call-normalization"

const COMMAND_FIELD_NAMES = new Set([
  "command",
  "cmd",
  "script",
  "shellcommand",
  "commandline",
  "commandtext",
  "commandargs",
  "executable",
  "program",
  "argv",
  "args",
  "cwd",
  "workdir",
  "workingdir",
  "workingdirectory",
])

const COMMAND_TOOL_NAME_RE =
  /(?:^|[.:/_-])(bash|sh|shell|exec|execute|command|terminal|run|process)(?:$|[.:/_-])/i
const COMMAND_FIELD_TEXT_RE =
  /"(?:command|cmd|script|shell[_-]?command|command[_-]?(?:line|text|args)|executable|program|argv|args|cwd|work(?:ing)?[_-]?dir(?:ectory)?)"\s*:/i

function normalizedCommandFieldName(key: string): string {
  return key.toLowerCase().replace(/[\s_-]+/g, "")
}

function containsCommandField(value: unknown, depth = 0): boolean {
  if (depth > 5 || value === null || typeof value !== "object") {
    return false
  }
  if (Array.isArray(value)) {
    return value.some((item) => containsCommandField(item, depth + 1))
  }
  const object = value as Record<string, unknown>
  return Object.entries(object).some(
    ([key, nested]) =>
      COMMAND_FIELD_NAMES.has(normalizedCommandFieldName(key)) ||
      containsCommandField(nested, depth + 1)
  )
}

/** Whether a tool's visible payload represents a command execution. */
export function isCommandLikeTool(
  toolName: string,
  input: string | null
): boolean {
  const normalizedName = normalizeToolName(toolName).toLowerCase()
  if (
    normalizedName === "bash" ||
    normalizedName === "exec_command" ||
    COMMAND_TOOL_NAME_RE.test(normalizedName)
  ) {
    return true
  }
  if (!input) return false

  try {
    return containsCommandField(JSON.parse(input))
  } catch {
    return COMMAND_FIELD_TEXT_RE.test(input)
  }
}

function isAbsoluteDisplayPath(value: string): boolean {
  return (
    /^[A-Za-z]:[\\/]/.test(value) ||
    /^\\\\/.test(value) ||
    /^\/[^/]+/.test(value) ||
    /^~[\\/]/.test(value) ||
    /^\$\{?HOME\}?[\\/]/i.test(value) ||
    /^%[^%]+%[\\/]/.test(value)
  )
}

function pathLeaf(value: string): string {
  const withoutTrailingSeparators = value.replace(/[\\/]+$/, "")
  return withoutTrailingSeparators.split(/[\\/]/).pop() ?? value
}

/** Hide absolute path prefixes while retaining the executable/file name. */
export function sanitizeCommandDisplayText(text: string): string {
  const quotedPathRe =
    /(["'`])((?:[A-Za-z]:[\\/]|\\\\|\/|~[\\/]|\$\{?HOME\}?[\\/]|%[^%]+%[\\/])[^"'`]*?)\1/g
  const windowsExecutablePathRe =
    /(?:[A-Za-z]:[\\/]|\\\\)[^"'`\r\n]*?[\\/][^"'`\r\n]*?\.(?:exe|cmd|bat|com|ps1|sh|py|js|mjs|cjs)(?=\s|$|["'`,;:)\]])/gi
  const unquotedPathRe =
    /(?<![\w"'`])(?:[A-Za-z]:[\\/](?:[^\\/\s"'`]+[\\/])*[^\\/\s"'`]+|\\\\[^\\/\s"'`]+(?:[\\/][^\\/\s"'`]+)+|\/(?:[^\/\s"'`]+[\/])+[^\/\s"'`]+|~[\\/](?:[^\\/\s"'`]+[\\/])*[^\\/\s"'`]+|\$\{?HOME\}?[\\/](?:[^\\/\s"'`]+[\\/])*[^\\/\s"'`]+|%[^%]+%[\\/](?:[^\\/\s"'`]+[\\/])*[^\\/\s"'`]+)/g

  const quoted = text.replace(
    quotedPathRe,
    (match, quote: string, path: string) =>
      isAbsoluteDisplayPath(path) ? `${quote}${pathLeaf(path)}${quote}` : match
  )
  const executables = quoted.replace(windowsExecutablePathRe, (match) =>
    pathLeaf(match)
  )
  return executables.replace(unquotedPathRe, (match) => pathLeaf(match))
}

export function sanitizeCommandDisplayValue(
  value: unknown,
  depth = 0
): unknown {
  if (depth > 8 || value === null || value === undefined) return value
  if (typeof value === "string") return sanitizeCommandDisplayText(value)
  if (typeof value !== "object") return value
  if (Array.isArray(value)) {
    return value.map((item) => sanitizeCommandDisplayValue(item, depth + 1))
  }
  if (Object.prototype.toString.call(value) !== "[object Object]") {
    return value
  }
  return Object.fromEntries(
    Object.entries(value as Record<string, unknown>).map(([key, nested]) => [
      key,
      sanitizeCommandDisplayValue(nested, depth + 1),
    ])
  )
}

/** Sanitize a JSON or plain-text command payload for display only. */
export function sanitizeCommandDisplayPayload(raw: string): string {
  try {
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed === "string") {
      return sanitizeCommandDisplayText(parsed)
    }
    return JSON.stringify(sanitizeCommandDisplayValue(parsed), null, 2) ?? ""
  } catch {
    return sanitizeCommandDisplayText(raw)
  }
}
