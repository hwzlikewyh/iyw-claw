export interface GatewayModel {
  id: string
  name: string
  description: string | null
  efforts: string[]
  defaultEffort: string | null
  fastModeSupported: boolean
  fastModeDefaultEnabled: boolean
  capabilities: GatewayModelCapabilities
  imageInputMode: GatewayImageInputMode
}

export interface GatewayModelCapabilities {
  streaming: boolean
  toolCalling: boolean
  parallelToolCalling: boolean
  webSearch: boolean
  vision: boolean
  audioInput: boolean
  structuredOutput: boolean
  promptCache: boolean
  imageGeneration: boolean
  imageEditing: boolean
}

export type GatewayImageInputMode = "native" | "fallback" | "none"

function uniqueStrings(value: unknown): string[] {
  if (!Array.isArray(value)) return []
  return Array.from(
    new Set(
      value.flatMap((item) => {
        if (typeof item !== "string") return []
        const trimmed = item.trim()
        return trimmed ? [trimmed] : []
      })
    )
  )
}

function booleanField(value: Record<string, unknown>, key: string): boolean {
  return value[key] === true
}

function parseCapabilities(value: unknown): GatewayModelCapabilities {
  const raw = value && typeof value === "object" ? value : {}
  const capabilities = raw as Record<string, unknown>
  return {
    streaming: booleanField(capabilities, "streaming"),
    toolCalling: booleanField(capabilities, "tool_calling"),
    parallelToolCalling: booleanField(capabilities, "parallel_tool_calling"),
    webSearch: booleanField(capabilities, "web_search"),
    vision: booleanField(capabilities, "vision"),
    audioInput: booleanField(capabilities, "audio_input"),
    structuredOutput: booleanField(capabilities, "structured_output"),
    promptCache: booleanField(capabilities, "prompt_cache"),
    imageGeneration: booleanField(capabilities, "image_generation"),
    imageEditing: booleanField(capabilities, "image_editing"),
  }
}

function parseImageInputMode(
  value: unknown,
  capabilities: GatewayModelCapabilities
): GatewayImageInputMode {
  const raw = value && typeof value === "object" ? value : {}
  const mode = (raw as Record<string, unknown>).mode
  if (mode === "native" || mode === "fallback" || mode === "none") {
    return mode
  }
  return capabilities.vision ? "native" : "none"
}

function parseGatewayModel(value: unknown): GatewayModel | null {
  if (!value || typeof value !== "object") return null
  const raw = value as Record<string, unknown>
  const id = typeof raw.id === "string" ? raw.id.trim() : ""
  if (!id) return null
  const reasoning =
    raw.reasoning && typeof raw.reasoning === "object"
      ? (raw.reasoning as Record<string, unknown>)
      : {}
  const efforts = uniqueStrings(reasoning.efforts)
  const defaultEffort =
    typeof reasoning.default_effort === "string" &&
    reasoning.default_effort.trim()
      ? reasoning.default_effort.trim()
      : null
  const fastMode =
    raw.fast_mode && typeof raw.fast_mode === "object"
      ? (raw.fast_mode as Record<string, unknown>)
      : {}
  const capabilities = parseCapabilities(raw.capabilities)
  return {
    id,
    name:
      typeof raw.display_name === "string" && raw.display_name.trim()
        ? raw.display_name.trim()
        : id,
    description:
      typeof raw.description === "string" && raw.description.trim()
        ? raw.description.trim()
        : null,
    efforts,
    defaultEffort,
    fastModeSupported: fastMode.supported === true,
    fastModeDefaultEnabled: fastMode.default_enabled === true,
    capabilities,
    imageInputMode: parseImageInputMode(raw.image_input, capabilities),
  }
}

export function parseGatewayModels(payload: unknown): GatewayModel[] {
  if (!payload || typeof payload !== "object") return []
  const data = (payload as { data?: unknown }).data
  if (!Array.isArray(data)) return []
  const seen = new Set<string>()
  return data.flatMap((item) => {
    const model = parseGatewayModel(item)
    if (!model || seen.has(model.id)) return []
    seen.add(model.id)
    return [model]
  })
}
