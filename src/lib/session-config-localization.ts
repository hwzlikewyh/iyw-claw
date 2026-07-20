import type {
  SessionConfigOptionInfo,
  SessionConfigSelectOptionInfo,
} from "@/lib/types"
import { isModelConfigOption } from "@/lib/model-config-groups"

export type SessionConfigTranslator = (key: string) => string

type ConfigDomain =
  | "mode"
  | "model"
  | "reasoning"
  | "responseMode"
  | "webSearch"
  | "switch"

interface LocalizedValue {
  name: string
  description?: string | null
}

const MODE_VALUE_KEYS: Record<string, string> = {
  "read-only": "readOnly",
  agent: "agent",
  "agent-full-access": "agentFullAccess",
}

const MODE_NAME_KEYS: Record<string, string> = {
  "read-only": "readOnly",
  agent: "agent",
  "agent (full access)": "agentFullAccess",
}

const REASONING_VALUE_KEYS: Record<string, string> = {
  off: "off",
  minimal: "minimal",
  low: "low",
  medium: "medium",
  high: "high",
  xhigh: "xhigh",
  "extra-high": "xhigh",
  extra_high: "xhigh",
}

const SWITCH_VALUE_KEYS: Record<string, string> = {
  off: "off",
  on: "on",
  true: "on",
  false: "off",
  enabled: "on",
  disabled: "off",
}

const RESPONSE_MODE_VALUE_KEYS: Record<string, string> = {
  off: "standard",
  default: "standard",
  standard: "standard",
  on: "fast",
  fast: "fast",
}

function translateOrFallback(
  t: SessionConfigTranslator,
  key: string,
  fallback: string
): string {
  try {
    const value = t(key)
    return value && value !== key ? value : fallback
  } catch {
    return fallback
  }
}

function normalized(raw: string | null | undefined): string {
  return (raw ?? "").trim().toLowerCase()
}

function optionDomain(option: SessionConfigOptionInfo): ConfigDomain | null {
  if (isModelConfigOption(option)) return "model"

  const id = normalized(option.id)
  const category = normalized(option.category)
  const name = normalized(option.name)
  const lookup = `${id} ${category} ${name}`

  if (id === "mode" || category === "mode" || lookup.includes("approval")) {
    return "mode"
  }
  if (
    id === "fast-mode" ||
    id === "fast" ||
    id === "fast_mode" ||
    lookup.includes("fast mode")
  ) {
    return "responseMode"
  }
  if (
    category === "thought_level" ||
    lookup.includes("reasoning") ||
    lookup.includes("thought") ||
    lookup.includes("effort")
  ) {
    return "reasoning"
  }
  if (lookup.includes("web_search") || lookup.includes("web search")) {
    return "webSearch"
  }
  return null
}

function localizeOptionName(
  option: SessionConfigOptionInfo,
  domain: ConfigDomain | null,
  t: SessionConfigTranslator
): string {
  if (!domain) return option.name
  return translateOrFallback(
    t,
    `sessionConfig.options.${domain}.name`,
    option.name
  )
}

function localizeOptionDescription(
  option: SessionConfigOptionInfo,
  domain: ConfigDomain | null,
  t: SessionConfigTranslator
): string | null | undefined {
  if (!domain || !option.description) return option.description
  return translateOrFallback(
    t,
    `sessionConfig.options.${domain}.description`,
    option.description
  )
}

function modeValueKey(option: SessionConfigSelectOptionInfo): string | null {
  const byValue = MODE_VALUE_KEYS[normalized(option.value)]
  if (byValue) return byValue
  return MODE_NAME_KEYS[normalized(option.name)] ?? null
}

function reasoningValueKey(
  option: SessionConfigSelectOptionInfo
): string | null {
  const byValue = REASONING_VALUE_KEYS[normalized(option.value)]
  if (byValue) return byValue
  return REASONING_VALUE_KEYS[normalized(option.name)] ?? null
}

function switchValueKey(option: SessionConfigSelectOptionInfo): string | null {
  const byValue = SWITCH_VALUE_KEYS[normalized(option.value)]
  if (byValue) return byValue
  return SWITCH_VALUE_KEYS[normalized(option.name)] ?? null
}

function responseModeValueKey(
  option: SessionConfigSelectOptionInfo
): string | null {
  const byValue = RESPONSE_MODE_VALUE_KEYS[normalized(option.value)]
  if (byValue) return byValue
  return RESPONSE_MODE_VALUE_KEYS[normalized(option.name)] ?? null
}

function localizeValue(
  option: SessionConfigSelectOptionInfo,
  domain: ConfigDomain | null,
  t: SessionConfigTranslator
): LocalizedValue {
  if (domain === "mode") {
    const key = modeValueKey(option)
    if (key) {
      return {
        name: translateOrFallback(
          t,
          `sessionConfig.values.mode.${key}.name`,
          option.name
        ),
        description: option.description
          ? translateOrFallback(
              t,
              `sessionConfig.values.mode.${key}.description`,
              option.description
            )
          : option.description,
      }
    }
  }

  if (domain === "reasoning") {
    const key = reasoningValueKey(option)
    if (key) {
      return {
        name: translateOrFallback(
          t,
          `sessionConfig.values.reasoning.${key}.name`,
          option.name
        ),
        description: option.description
          ? translateOrFallback(
              t,
              `sessionConfig.values.reasoning.${key}.description`,
              option.description
            )
          : option.description,
      }
    }
  }

  if (domain === "responseMode") {
    const key = responseModeValueKey(option)
    if (key) {
      return {
        name: translateOrFallback(
          t,
          `sessionConfig.values.responseMode.${key}.name`,
          option.name
        ),
        description: option.description
          ? translateOrFallback(
              t,
              `sessionConfig.values.responseMode.${key}.description`,
              option.description
            )
          : option.description,
      }
    }
  }

  if (domain === "webSearch" || domain === "switch" || domain === null) {
    const key = switchValueKey(option)
    if (key) {
      return {
        name: translateOrFallback(
          t,
          `sessionConfig.values.switch.${key}`,
          option.name
        ),
        description: option.description
          ? translateOrFallback(
              t,
              `sessionConfig.values.switch.${key}Description`,
              option.description
            )
          : option.description,
      }
    }
  }

  return { name: option.name, description: option.description }
}

function localizeSelectOption(
  option: SessionConfigSelectOptionInfo,
  domain: ConfigDomain | null,
  t: SessionConfigTranslator
): SessionConfigSelectOptionInfo {
  const localized = localizeValue(option, domain, t)
  return {
    ...option,
    name: localized.name,
    description: localized.description,
  }
}

export function localizeSessionConfigOption(
  option: SessionConfigOptionInfo,
  t: SessionConfigTranslator
): SessionConfigOptionInfo {
  const baseDomain = optionDomain(option)
  const valueDomain = baseDomain ?? "switch"
  if (option.kind.type !== "select") return option

  return {
    ...option,
    name: localizeOptionName(option, baseDomain, t),
    description: localizeOptionDescription(option, baseDomain, t),
    kind: {
      ...option.kind,
      options: option.kind.options.map((item) =>
        localizeSelectOption(item, valueDomain, t)
      ),
      groups: option.kind.groups.map((group) => ({
        ...group,
        options: group.options.map((item) =>
          localizeSelectOption(item, valueDomain, t)
        ),
      })),
    },
  }
}
