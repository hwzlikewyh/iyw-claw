import type {
  ContentBlock,
  MessageTurn,
  SessionFailureRecord,
} from "@/lib/types"

/** Merge one AIR failure upsert using the per-id monotonic revision rule. */
export function upsertSessionFailure(
  current: SessionFailureRecord[],
  record: SessionFailureRecord
): SessionFailureRecord[] {
  return mergeSessionFailures(current, [record])
}

/**
 * Merge live or hydrated AIR failure upserts. Equal revisions may only carry
 * a client-inferred resolution forward; a replay can never reopen a record.
 */
export function mergeSessionFailures(
  current: SessionFailureRecord[],
  incoming: SessionFailureRecord[] | null | undefined
): SessionFailureRecord[] {
  if (!incoming || incoming.length === 0) return current

  let next: SessionFailureRecord[] | null = null
  for (const record of incoming) {
    if (!record.id || !(record.revision >= 1)) continue

    const target = next ?? current
    const index = target.findIndex((failure) => failure.id === record.id)
    const stored = index >= 0 ? target[index] : null
    if (stored && record.revision < stored.revision) continue

    if (stored && record.revision === stored.revision) {
      if (!record.resolved || stored.resolved) continue
      next ??= [...current]
      const at = next.findIndex((failure) => failure.id === record.id)
      next[at] = { ...next[at], resolved: true }
      continue
    }

    next ??= [...current]
    const accepted = { ...record, resolved: record.resolved ?? false }
    const nextIndex = next.findIndex((failure) => failure.id === record.id)
    if (nextIndex >= 0) next[nextIndex] = accepted
    else next.push(accepted)
  }
  return next ?? current
}

export type SessionFailureSettleScope = "retry_incidents" | "warnings" | "all"

function isRetryIncident(failure: SessionFailureRecord): boolean {
  return failure.severity === "warning" && failure.category !== "unknown"
}

export function hasSettleableRetryIncident(
  failures: SessionFailureRecord[]
): boolean {
  return failures.some(
    (failure) => !failure.resolved && isRetryIncident(failure)
  )
}

/** Settle failures inferred as recovered at a lifecycle boundary. */
export function settleSessionFailures(
  failures: SessionFailureRecord[],
  scope: SessionFailureSettleScope
): SessionFailureRecord[] {
  const settles = (failure: SessionFailureRecord) => {
    if (failure.resolved) return false
    if (scope === "all") return true
    if (scope === "warnings") return failure.severity === "warning"
    return isRetryIncident(failure)
  }
  if (!failures.some(settles)) return failures
  return failures.map((failure) =>
    settles(failure) ? { ...failure, resolved: true } : failure
  )
}

/** Dismiss all records represented by a collapsed failure strip. */
export function dismissSessionFailures(
  failures: SessionFailureRecord[],
  ids: string[]
): SessionFailureRecord[] {
  const targets = new Set(ids)
  const changes = (failure: SessionFailureRecord) =>
    targets.has(failure.id) && !(failure.resolved && failure.dismissed)

  if (!failures.some(changes)) return failures
  return failures.map((failure) =>
    changes(failure) ? { ...failure, resolved: true, dismissed: true } : failure
  )
}

export function activeSessionFailures(
  failures: SessionFailureRecord[]
): SessionFailureRecord[] {
  return failures.filter((failure) => !failure.resolved)
}

export interface ActiveSessionFailureView {
  errors: SessionFailureRecord[]
  warning: SessionFailureRecord | null
  hiddenWarnings: number
  warningIds: string[]
}

/** Collapse transient warnings while preserving every terminal error. */
export function activeSessionFailureView(
  failures: SessionFailureRecord[]
): ActiveSessionFailureView {
  const active = activeSessionFailures(failures)
  const warnings = active.filter((failure) => failure.severity === "warning")
  return {
    errors: active.filter((failure) => failure.severity !== "warning"),
    warning: warnings[warnings.length - 1] ?? null,
    hiddenWarnings: Math.max(0, warnings.length - 1),
    warningIds: warnings.map((failure) => failure.id),
  }
}

export function mostRecentRecoveredWarning(
  failures: SessionFailureRecord[]
): SessionFailureRecord | null {
  for (let index = failures.length - 1; index >= 0; index--) {
    const failure = failures[index]
    if (
      failure.resolved &&
      !failure.dismissed &&
      failure.severity === "warning"
    ) {
      return failure
    }
  }
  return null
}

export function resolvedSessionFailures(
  failures: SessionFailureRecord[]
): SessionFailureRecord[] {
  return failures.filter((failure) => failure.resolved)
}

/** Find the latest retryable user prompt from runtime or persisted turns. */
export function lastUserPromptText(
  turns: MessageTurn[] | undefined
): string | null {
  if (!turns) return null
  for (let index = turns.length - 1; index >= 0; index--) {
    const turn = turns[index]
    if (turn.role !== "user") continue
    const text = turn.blocks
      .filter(
        (block): block is Extract<ContentBlock, { type: "text" }> =>
          block.type === "text"
      )
      .map((block) => block.text)
      .join("\n")
      .trim()
    if (text) return text
  }
  return null
}

export const KNOWN_SESSION_FAILURE_ACTIONS = [
  "retry",
  "login",
  "new_session",
] as const

export type SessionFailureAction =
  (typeof KNOWN_SESSION_FAILURE_ACTIONS)[number]

export function knownSessionFailureActions(
  record: SessionFailureRecord
): SessionFailureAction[] {
  const actions = record.actions ?? []
  return KNOWN_SESSION_FAILURE_ACTIONS.filter((action) =>
    actions.includes(action)
  )
}
