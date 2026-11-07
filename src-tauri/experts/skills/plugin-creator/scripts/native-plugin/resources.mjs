const MAX_TEMPLATES = 256
const MAX_URI_BYTES = 2048
const MAX_SEGMENTS = 64

export async function readResourceTemplates(client, enabled) {
  if (!enabled) return []
  let templates
  try {
    templates = await client.list("resources/templates/list", "resourceTemplates")
  } catch (cause) {
    if (cause instanceof Error && cause.message.startsWith("MCP error -32601:")) return []
    throw cause
  }
  const ui = templates.filter((value) => value.uriTemplate?.startsWith("ui://"))
  if (ui.length > MAX_TEMPLATES || ui.some((value) => !validTemplate(value.uriTemplate)))
    throw Error("Unsupported UI resource template")
  return ui
}

function validTemplate(uri) {
  if (Buffer.byteLength(uri) > MAX_URI_BYTES || /[^\x21-\x7e]|[%?#\\]/.test(uri)) return false
  const parts = uri.slice("ui://".length).split("/")
  if (parts.length < 2 || parts.length > MAX_SEGMENTS || /[{}]/.test(parts[0])) return false
  return parts.every((part, index) => {
    if (!part || part === "." || part === "..") return false
    if (!/[{}]/.test(part)) return true
    const parameter = /^\{(\+?)[A-Za-z0-9_]+\}$/.exec(part)
    return !!parameter && (!parameter[1] || index + 1 === parts.length)
  })
}
