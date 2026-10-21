const CLI_HEADER_LINE =
  /^(?:chunk id|wall time|original token count|total output lines|exit code)\s*[:=]|^process (?:exited with code|running with session id)\s/i

export function commandExitCode(raw?: string | null): number | null {
  if (!raw) return null
  let header = raw
  try {
    const value: unknown = JSON.parse(raw)
    if (value && typeof value === "object" && !Array.isArray(value)) {
      const record = value as Record<string, unknown>
      if (
        typeof record.exit_code === "number" &&
        Number.isInteger(record.exit_code)
      )
        return record.exit_code
      if (typeof record.formatted_output === "string")
        header = record.formatted_output
    }
  } catch {
    // 非 JSON 输出只识别 CLI 开头的元数据，避免把日志正文误判为退出状态。
  }
  for (const line of header.split(/\r?\n/)) {
    const text = line.trim()
    if (!text) continue
    const match = text.match(
      /^(?:process exited with code\s+|exit code\s*[:=]\s*)(-?\d+)\s*$/i
    )
    if (match) return Number(match[1])
    if (!CLI_HEADER_LINE.test(text)) break
  }
  return null
}
