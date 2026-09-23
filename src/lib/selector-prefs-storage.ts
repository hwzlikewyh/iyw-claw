"use client"

/**
 * Persists user's mode and non-model config selections per agentType, plus
 * reasoning selections per model, to localStorage. Model selection belongs to
 * the conversation and is stored in `conversation.model` after ACP confirms it.
 *
 * Structure hash is stored alongside values — when the saved value no
 * longer exists in the current option set (item renamed / removed) the
 * backend's `set_session_config_option` will reject the application and
 * the stale value is naturally dropped on the next user pick.
 *
 * Preferences are shipped to the backend at `acp_connect` time (see
 * `getSavedPrefsForConnect`) which applies them to the agent BEFORE
 * the initial `session_modes` / `session_config_options` events are
 * emitted. Snapshots, replays, and live events therefore all carry the
 * user-preferred values uniformly — there is no client-side "intercept
 * incoming event and overwrite locally" path.
 */

import { automaticAgentMode } from "@/lib/automatic-agent-mode"
import { getAgentModeState } from "@/lib/agent-modes"
import type { AgentType, SessionModeStateInfo } from "@/lib/types"
import { loadConversationDisplayPreferences } from "@/lib/conversation-display-preferences"

const STORAGE_KEY = "iyw-claw:selector-prefs"

interface SelectorPrefs {
  modeId?: string
  configValues?: Record<string, string>
  modelConfigValues?: Record<string, Record<string, string>>
}

type AllPrefs = Record<string, SelectorPrefs>

function readAll(): AllPrefs {
  if (typeof window === "undefined") return {}
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    return raw ? (JSON.parse(raw) as AllPrefs) : {}
  } catch {
    return {}
  }
}

function writeAll(all: AllPrefs) {
  if (typeof window === "undefined") return
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(all))
  } catch {
    /* ignore */
  }
}

function withoutSessionModel(
  configValues?: Record<string, string>
): Record<string, string> | undefined {
  if (!configValues) return undefined
  const next = { ...configValues }
  delete next.model
  return Object.keys(next).length > 0 ? next : undefined
}

function isReasoningConfigId(configId: string): boolean {
  const normalized = configId.trim().toLowerCase().replace(/_/g, "-")
  return (
    normalized.includes("reasoning") ||
    normalized.includes("thought") ||
    normalized.includes("effort")
  )
}

function withoutGlobalReasoning(
  configValues?: Record<string, string>
): Record<string, string> | undefined {
  const next = withoutSessionModel(configValues)
  if (!next) return undefined
  for (const configId of Object.keys(next)) {
    if (isReasoningConfigId(configId)) delete next[configId]
  }
  return Object.keys(next).length > 0 ? next : undefined
}

function normalizeModelConfigValues(
  modelConfigValues?: Record<string, Record<string, string>>
): Record<string, Record<string, string>> | undefined {
  if (!modelConfigValues) return undefined
  const next: Record<string, Record<string, string>> = {}
  for (const [modelId, values] of Object.entries(modelConfigValues)) {
    if (!modelId || !values || typeof values !== "object") continue
    const normalizedValues: Record<string, string> = {}
    for (const [configId, valueId] of Object.entries(values)) {
      if (
        isReasoningConfigId(configId) &&
        typeof valueId === "string" &&
        valueId.trim()
      ) {
        normalizedValues[configId] = valueId
      }
    }
    if (Object.keys(normalizedValues).length > 0) {
      next[modelId] = normalizedValues
    }
  }
  return Object.keys(next).length > 0 ? next : undefined
}

function updatePrefs(
  agentType: string,
  fn: (prefs: SelectorPrefs) => SelectorPrefs
) {
  const all = readAll()
  const existing = all[agentType]
  // Re-project onto the current schema so legacy fields (`modesHash` /
  // `configHash` from before the backend took ownership of preference
  // application) don't survive across writes. Without this an upgrade
  // user's first save would re-persist the stale hash bytes forever.
  const normalized: SelectorPrefs = {
    modeId: existing?.modeId,
    configValues: withoutGlobalReasoning(existing?.configValues),
    modelConfigValues: normalizeModelConfigValues(existing?.modelConfigValues),
  }
  all[agentType] = fn(normalized)
  writeAll(all)
}

// ── Read ──

function resolveModeId(
  agentType: AgentType,
  prefs?: SelectorPrefs
): string | null {
  const normalized = normalizeLegacyPrefs(agentType, prefs)
  const saved = normalized.modeId
  const modes = getAgentModeState(agentType).available_modes
  if (saved && modes.some(({ id }) => id === saved)) return saved
  return automaticAgentMode(agentType)?.id ?? null
}

const PI_THINKING_LEVELS = new Set([
  "off",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
])

function normalizeLegacyPrefs(
  agentType: AgentType,
  prefs?: SelectorPrefs
): SelectorPrefs {
  const next: SelectorPrefs = {
    modeId: prefs?.modeId,
    configValues: { ...prefs?.configValues },
    modelConfigValues: normalizeModelConfigValues(prefs?.modelConfigValues),
  }
  if (agentType === "codex" && next.modeId === "plan") {
    next.modeId = "agent"
    next.configValues!.collaboration_mode ??= "plan"
  }
  const legacyPiMode = next.modeId
  if (agentType === "pi" && PI_THINKING_LEVELS.has(legacyPiMode ?? "")) {
    next.configValues!.thought_level ??= legacyPiMode!
    next.modeId = undefined
  }
  return next
}

/** Read the saved mode, falling back to the product automatic mode. */
export function getSavedModeId(agentType: AgentType): string | null {
  const all = readAll()
  return resolveModeId(agentType, all[agentType])
}

/**
 * Read all saved preferences for an agent. Returned shape mirrors what
 * the backend `acp_connect` command accepts (`preferred_mode_id` +
 * `preferred_config_values`). Null/empty fields are normalized so the
 * call site can pass the result through unchanged.
 *
 * The backend applies these on the freshly-attached session before any
 * `session_modes` / `session_config_options` event is emitted, so the
 * frontend never needs to "intercept event and overwrite, then sync back".
 */
export function getSavedPrefsForConnect(agentType: AgentType): {
  modeId: string | null
  configValues: Record<string, string> | null
} {
  const all = readAll()
  const prefs = normalizeLegacyPrefs(agentType, all[agentType])

  const configValues = {
    ...withoutGlobalReasoning(prefs.configValues),
    __iyw_response_style: loadConversationDisplayPreferences().responseStyle,
  }
  return {
    modeId: resolveModeId(agentType, prefs),
    configValues: Object.keys(configValues).length > 0 ? configValues : null,
  }
}

/** Read all valid model-scoped reasoning preferences for the agent. */
export function getSavedModelConfigPreferences(
  agentType: AgentType
): Record<string, Record<string, string>> {
  const all = readAll()
  return normalizeModelConfigValues(all[agentType]?.modelConfigValues) ?? {}
}

/** Save a reasoning preference for one concrete model. */
export function saveModelConfigPreference(
  agentType: string,
  modelId: string,
  configId: string,
  valueId: string
) {
  const normalizedModelId = modelId.trim()
  if (!normalizedModelId || !isReasoningConfigId(configId) || !valueId.trim()) {
    return
  }
  updatePrefs(agentType, (prefs) => ({
    ...prefs,
    modelConfigValues: {
      ...prefs.modelConfigValues,
      [normalizedModelId]: {
        ...prefs.modelConfigValues?.[normalizedModelId],
        [configId]: valueId,
      },
    },
  }))
}

// ── Save (user actions only) ──

export function saveModePreference(
  agentType: string,
  modes: SessionModeStateInfo
) {
  updatePrefs(agentType, (prefs) => ({
    ...prefs,
    modeId: modes.current_mode_id,
  }))
}

export function saveConfigPreference(
  agentType: string,
  configId: string,
  valueId: string
) {
  if (configId === "model" || isReasoningConfigId(configId)) return
  updatePrefs(agentType, (prefs) => ({
    ...prefs,
    configValues: { ...prefs.configValues, [configId]: valueId },
  }))
}

export function replaceConfigPreferences(
  agentType: string,
  configValues: Record<string, string>
) {
  const next = withoutGlobalReasoning(configValues)
  updatePrefs(agentType, (prefs) => ({
    ...prefs,
    configValues: next,
  }))
}
