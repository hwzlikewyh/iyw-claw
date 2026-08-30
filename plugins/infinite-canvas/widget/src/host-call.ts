import type { App } from "@modelcontextprotocol/ext-apps"

export async function callCanvasTool(app: App, name: string, args: Record<string, unknown>): Promise<Record<string, unknown>> {
  const result = await app.callServerTool({ name, arguments: args })
  const raw = readText(result.content)
  if (result.isError) throw parseError(raw || "Canvas tool failed")
  return JSON.parse(raw || "{}") as Record<string, unknown>
}

function parseError(raw: string): Error & { code?: string; details?: Record<string, unknown> } {
  try {
    const payload = JSON.parse(raw) as { code?: string; message?: string; details?: Record<string, unknown> }
    const error = new Error(payload.message || raw) as Error & { code?: string; details?: Record<string, unknown> }
    error.code = payload.code
    error.details = payload.details
    return error
  } catch { return new Error(raw) }
}

function readText(content: readonly { type: string; text?: string }[] | undefined): string {
  return content?.find((item) => item.type === "text")?.text ?? ""
}
