import {
  getShellTransport,
  getTransport,
  isDesktop,
  isRemoteDesktopMode,
} from "./index"
import type { Transport } from "./types"

export type CapabilityDecision = {
  enabled: boolean
  denialCode?: string | null
}

export type AgentCapability =
  | "host_execution"
  | "host_read"
  | "host_write"
  | "terminal"
  | "mcp"

export type ClientCapability = "file_upload"
export type LocalCapability = AgentCapability | ClientCapability

type CapabilitySubject =
  | { subjectKind: "client"; subjectId: "global" }
  | { subjectKind: "agent"; subjectId: string }

type CapabilityRequest = CapabilitySubject & {
  capability: LocalCapability
}

export type CapabilityPreference = CapabilityRequest & {
  enabled: boolean
  updatedAt: string
}

export type ClientFileUploadPreference = {
  enabled: boolean
  mixed: boolean
}

export class CapabilityPolicyDeniedError extends Error {
  readonly code = "permission_denied"
  readonly detail: string

  constructor(detail: string) {
    super("Capability is disabled")
    this.name = "CapabilityPolicyDeniedError"
    this.detail = detail
  }
}

function transportArgs<T>(request: T, tauriCommand: boolean) {
  return tauriCommand ? { request } : request
}

function primaryTarget(): { transport: Transport; tauriCommand: boolean } {
  return {
    transport: getTransport(),
    tauriCommand: isDesktop() && !isRemoteDesktopMode(),
  }
}

function clientTargets(): Array<{
  transport: Transport
  tauriCommand: boolean
}> {
  const primary = primaryTarget()
  if (!isRemoteDesktopMode()) return [primary]
  return [primary, { transport: getShellTransport(), tauriCommand: true }]
}

async function listOnTransport(
  transport: Transport,
  tauriCommand: boolean,
  subject: CapabilitySubject
): Promise<CapabilityPreference[]> {
  return transport.call<CapabilityPreference[]>(
    "capability_preference_list",
    transportArgs(subject, tauriCommand)
  )
}

async function setOnTransport(
  transport: Transport,
  tauriCommand: boolean,
  request: CapabilityRequest & { enabled: boolean }
): Promise<CapabilityPreference> {
  return transport.call<CapabilityPreference>(
    "capability_preference_set",
    transportArgs(request, tauriCommand)
  )
}

async function decisionOnTransport(
  transport: Transport,
  tauriCommand: boolean,
  request: CapabilityRequest
): Promise<CapabilityDecision> {
  return transport.call<CapabilityDecision>(
    "capability_policy_decision",
    transportArgs(request, tauriCommand)
  )
}

async function requireOnTransport(
  transport: Transport,
  tauriCommand: boolean
): Promise<void> {
  const decision = await decisionOnTransport(transport, tauriCommand, {
    subjectKind: "client",
    subjectId: "global",
    capability: "file_upload",
  })
  if (!decision.enabled) {
    throw new CapabilityPolicyDeniedError(
      decision.denialCode ?? "remote_policy_denied"
    )
  }
}

export async function requireFileUploadCapability(): Promise<void> {
  await Promise.all(
    clientTargets().map(({ transport, tauriCommand }) =>
      requireOnTransport(transport, tauriCommand)
    )
  )
}

export async function getClientFileUploadPreference(): Promise<ClientFileUploadPreference> {
  const rows = await Promise.all(
    clientTargets().map(({ transport, tauriCommand }) =>
      listOnTransport(transport, tauriCommand, {
        subjectKind: "client",
        subjectId: "global",
      })
    )
  )
  const values = rows.map((items) =>
    items.some(
      (item) => item.capability === "file_upload" && item.enabled === true
    )
  )
  return {
    enabled: values.every(Boolean),
    mixed: values.some(Boolean) && !values.every(Boolean),
  }
}

export async function setClientFileUploadPreference(
  enabled: boolean
): Promise<ClientFileUploadPreference> {
  const request = {
    subjectKind: "client" as const,
    subjectId: "global" as const,
    capability: "file_upload" as const,
    enabled,
  }
  const results = await Promise.allSettled(
    clientTargets().map(({ transport, tauriCommand }) =>
      setOnTransport(transport, tauriCommand, request)
    )
  )
  const rejected = results.find(
    (result): result is PromiseRejectedResult => result.status === "rejected"
  )
  if (rejected) throw rejected.reason
  return getClientFileUploadPreference()
}

export async function getClientFileUploadDecision(): Promise<CapabilityDecision> {
  const decisions = await Promise.all(
    clientTargets().map(({ transport, tauriCommand }) =>
      decisionOnTransport(transport, tauriCommand, {
        subjectKind: "client",
        subjectId: "global",
        capability: "file_upload",
      })
    )
  )
  const denied = decisions.find((decision) => !decision.enabled)
  return denied ?? { enabled: true, denialCode: null }
}

export async function listAgentCapabilityPreferences(
  platformId: string
): Promise<Record<AgentCapability, boolean>> {
  const { transport, tauriCommand } = primaryTarget()
  const rows = await listOnTransport(transport, tauriCommand, {
    subjectKind: "agent",
    subjectId: platformId,
  })
  const result: Record<AgentCapability, boolean> = {
    host_execution: false,
    host_read: false,
    host_write: false,
    terminal: false,
    mcp: false,
  }
  for (const row of rows) {
    if (row.capability in result) {
      result[row.capability as AgentCapability] = row.enabled
    }
  }
  return result
}

export async function setAgentCapabilityPreference(
  platformId: string,
  capability: AgentCapability,
  enabled: boolean
): Promise<CapabilityPreference> {
  const { transport, tauriCommand } = primaryTarget()
  return setOnTransport(transport, tauriCommand, {
    subjectKind: "agent",
    subjectId: platformId,
    capability,
    enabled,
  })
}

export async function getAgentCapabilityDecision(
  platformId: string,
  capability: AgentCapability
): Promise<CapabilityDecision> {
  const { transport, tauriCommand } = primaryTarget()
  return decisionOnTransport(transport, tauriCommand, {
    subjectKind: "agent",
    subjectId: platformId,
    capability,
  })
}
