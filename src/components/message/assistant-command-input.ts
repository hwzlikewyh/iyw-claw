const SHELLS = new Set(["pwsh", "powershell", "cmd", "bash", "sh", "zsh"])
const MAX_SHELL_DEPTH = 3

export function commandTokens(value: string): string[] {
  return value.match(/"[^"\r\n]*"|'[^'\r\n]*'|[^\s|]+/g) ?? []
}

function unquote(value: string) {
  return value.replace(/^(["'])([\s\S]*)\1$/, "$2")
}

export function commandText(
  input: Record<string, unknown>,
  depth = 0
): string | null {
  const raw = input.cmd ?? input.command ?? input.script
  const tokens = Array.isArray(raw)
    ? raw.filter((part): part is string => typeof part === "string")
    : typeof raw === "string"
      ? commandTokens(raw)
      : []
  if (!tokens.length) return null
  const text = typeof raw === "string" ? raw : tokens.join(" ")
  if (depth >= MAX_SHELL_DEPTH) return text
  const executableIndex = tokens[0] === "&" ? 1 : 0
  const executable = unquote(tokens[executableIndex] ?? "")
    .split(/[\\/]/)
    .pop()
    ?.replace(/\.exe$/i, "")
    .toLowerCase()
  if (!executable || !SHELLS.has(executable)) return text
  const flag = tokens.findIndex((token) =>
    /^(?:-command|-commandwithargs|-[a-z]*c|\/c)$/i.test(token)
  )
  if (flag < 0 || !tokens[flag + 1]) return text
  // 只读取 shell 后的脚本文字供展示，不执行或展开变量。
  const script = unquote(tokens.slice(flag + 1).join(" "))
  return commandText({ command: script }, depth + 1)
}
