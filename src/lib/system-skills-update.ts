import { getTransport, type UnsubscribeFn } from "./transport"

export type SystemSkillsUpdateStatus =
  | "idle"
  | "checking"
  | "update_available"
  | "downloading"
  | "validating"
  | "applying"
  | "up_to_date"
  | "blocked_dirty"
  | "error"

export interface SystemSkillsUpdateState {
  seq: number
  status: SystemSkillsUpdateStatus
  currentVersion: string | null
  currentCommit: string | null
  previousVersion: string | null
  latestVersion: string | null
  autoUpdate: boolean
  lastCheckedAt: string | null
  dirty: boolean
  error: string | null
}

const TRANSFER_TIMEOUT_MS = 600_000

export function getSystemSkillsUpdateState() {
  return getTransport().call<SystemSkillsUpdateState>(
    "system_skills_update_state"
  )
}

export function checkSystemSkillsUpdate() {
  return getTransport().call<SystemSkillsUpdateState>(
    "system_skills_check_update",
    {},
    { timeoutMs: 30_000 }
  )
}

export function applySystemSkillsUpdate() {
  return getTransport().call<SystemSkillsUpdateState>(
    "system_skills_apply_update",
    {},
    { timeoutMs: TRANSFER_TIMEOUT_MS }
  )
}

export function setSystemSkillsAutoUpdate(enabled: boolean) {
  return getTransport().call<SystemSkillsUpdateState>(
    "system_skills_set_auto_update",
    { enabled }
  )
}

export function rollbackSystemSkillsUpdate() {
  return getTransport().call<SystemSkillsUpdateState>(
    "system_skills_rollback",
    {},
    { timeoutMs: TRANSFER_TIMEOUT_MS }
  )
}

export function subscribeSystemSkillsUpdate(
  handler: (state: SystemSkillsUpdateState) => void
): Promise<UnsubscribeFn> {
  return getTransport().subscribe("system_skills_update_state", handler)
}
