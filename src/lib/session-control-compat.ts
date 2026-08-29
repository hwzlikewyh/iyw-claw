import type { SessionConfigOptionInfo, SessionModeStateInfo } from "@/lib/types"

export type SessionControlMismatchKind =
  | "missing_modes"
  | "missing_mode_value"
  | "unknown_live_mode"
  | "missing_config"
  | "missing_config_value"
  | "unknown_live_config"

export interface SessionControlMismatch {
  kind: SessionControlMismatchKind
  controlId: string
  valueId?: string
}

function matchingLiveConfig(
  fixed: SessionConfigOptionInfo,
  live: SessionConfigOptionInfo[]
): SessionConfigOptionInfo | null {
  const direct = live.find(({ id }) => id === fixed.id)
  if (direct) return direct
  if (!fixed.category) return null
  const semantic = live.filter(({ category }) => category === fixed.category)
  return semantic.length === 1 ? semantic[0] : null
}

function compareModes(
  fixed: SessionModeStateInfo | null,
  live: SessionModeStateInfo | null
): SessionControlMismatch[] {
  const expected = new Set(fixed?.available_modes.map(({ id }) => id) ?? [])
  if (expected.size === 0) return []
  const actual = new Set(live?.available_modes.map(({ id }) => id) ?? [])
  if (expected.size > 0 && !live) {
    return [{ kind: "missing_modes", controlId: "session_mode" }]
  }
  const missing = [...expected]
    .filter((id) => !actual.has(id))
    .map((valueId) => ({
      kind: "missing_mode_value" as const,
      controlId: "session_mode",
      valueId,
    }))
  const unknown = [...actual]
    .filter((id) => !expected.has(id))
    .map((valueId) => ({
      kind: "unknown_live_mode" as const,
      controlId: "session_mode",
      valueId,
    }))
  return [...missing, ...unknown]
}

function compareConfig(
  fixed: SessionConfigOptionInfo[],
  live: SessionConfigOptionInfo[]
): SessionControlMismatch[] {
  const matchedLiveIds = new Set<string>()
  const mismatches: SessionControlMismatch[] = fixed.flatMap(
    (control): SessionControlMismatch[] => {
      const matched = matchingLiveConfig(control, live)
      if (!matched) {
        return [{ kind: "missing_config" as const, controlId: control.id }]
      }
      matchedLiveIds.add(matched.id)
      const expected = new Set(control.kind.options.map(({ value }) => value))
      const actual = new Set(matched.kind.options.map(({ value }) => value))
      const missing = [...expected]
        .filter((valueId) => !actual.has(valueId))
        .map((valueId) => ({
          kind: "missing_config_value" as const,
          controlId: control.id,
          valueId,
        }))
      const unknown = [...actual]
        .filter((valueId) => !expected.has(valueId))
        .map((valueId) => ({
          kind: "unknown_live_config" as const,
          controlId: control.id,
          valueId,
        }))
      return [...missing, ...unknown]
    }
  )
  for (const control of live) {
    if (matchedLiveIds.has(control.id)) continue
    mismatches.push({
      kind: "unknown_live_config",
      controlId: control.id,
    })
  }
  return mismatches
}

export function compareSessionControlInventory(args: {
  fixedModes: SessionModeStateInfo | null
  liveModes: SessionModeStateInfo | null
  fixedConfig: SessionConfigOptionInfo[]
  liveConfig: SessionConfigOptionInfo[] | null
}): SessionControlMismatch[] {
  const liveConfig = (args.liveConfig ?? []).filter(
    (option) =>
      !(
        args.fixedModes &&
        args.fixedModes.available_modes.length > 0 &&
        (option.id === "mode" || option.category === "mode")
      )
  )
  return [
    ...compareModes(args.fixedModes, args.liveModes),
    ...compareConfig(args.fixedConfig, liveConfig),
  ]
}
