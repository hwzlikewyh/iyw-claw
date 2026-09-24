"use client"

import { getTransport } from "@/lib/transport"
import type { Transport } from "@/lib/transport"
import type { AgentType } from "@/lib/types"
import { getSavedPrefsForConnect } from "@/lib/selector-prefs-storage"

const PREPARATION_TTL_MS = 110_000
const MAX_PREPARATIONS = 2
const HOVER_DELAY_MS = 150

interface Target {
  agentType: AgentType
  workingDir?: string
  sessionId?: string
  conversationId?: number
}

interface Handle {
  id: string
  workingDir: string
}

interface Entry {
  key: string
  target: Target
  transport: Transport
  createdAt: number
  promise: Promise<Handle | null>
  timer: ReturnType<typeof setTimeout>
  reservedWorkingDir?: string
}

const entries = new Map<string, Entry>()

function keyFor(target: Target): string {
  const prefs = getSavedPrefsForConnect(target.agentType)
  return JSON.stringify({
    ...(target.conversationId
      ? { agentType: target.agentType, conversationId: target.conversationId }
      : target),
    modeId: prefs.modeId,
    configValues: Object.entries(prefs.configValues ?? {}).sort(([a], [b]) =>
      a.localeCompare(b)
    ),
  })
}

function retire(entry: Entry) {
  if (entries.get(entry.key) !== entry) return
  entries.delete(entry.key)
  clearTimeout(entry.timer)
  void entry.promise.then((handle) => {
    if (handle) {
      void entry.transport
        .call("acp_cancel_prepared_session", { preparationId: handle.id })
        .catch(() => {})
    }
  })
}

export function prepareAcpSession(target: Target): Promise<Handle | null> {
  if (target.agentType !== "codex" && target.agentType !== "claude_code") {
    return Promise.resolve(null)
  }
  const transport = getTransport()
  const key = keyFor(target)
  const current = entries.get(key)
  if (
    current?.transport === transport &&
    Date.now() - current.createdAt < PREPARATION_TTL_MS
  ) {
    return current.promise
  }
  for (const entry of entries.values()) {
    if (
      entry.transport !== transport ||
      Boolean(entry.target.conversationId) === Boolean(target.conversationId)
    ) {
      retire(entry)
    }
  }
  if (entries.size >= MAX_PREPARATIONS) return Promise.resolve(null)
  const promise = requestPreparation(target, transport)
  const entry: Entry = {
    key,
    target,
    transport,
    createdAt: Date.now(),
    promise,
    timer: setTimeout(() => retire(entry), PREPARATION_TTL_MS),
  }
  entries.set(key, entry)
  void promise.then((handle) => {
    if (!handle && entries.get(key) === entry) {
      entries.delete(key)
      clearTimeout(entry.timer)
    }
  })
  return promise
}

function requestPreparation(target: Target, transport: Transport) {
  const prefs = getSavedPrefsForConnect(target.agentType)
  return transport
    .call<Handle | null>("acp_prepare_session", {
      request: {
        agentType: target.agentType,
        workingDir: target.workingDir ?? null,
        sessionId: target.sessionId ?? null,
        conversationId: target.conversationId ?? null,
        preferredModeId: prefs.modeId,
        preferredConfigValues: prefs.configValues ?? {},
      },
    })
    .catch(() => null)
}

export async function reservePreparedChatDir(
  agentType: AgentType
): Promise<string | null> {
  const key = keyFor({ agentType })
  const entry = entries.get(key)
  if (!entry || entry.transport !== getTransport()) return null
  const handle = await entry.promise
  if (!handle) return null
  const workingDir = await entry.transport
    .call<string | null>("acp_reserve_prepared_workspace", {
      preparationId: handle.id,
    })
    .catch(() => null)
  if (workingDir) entry.reservedWorkingDir = workingDir
  return workingDir
}

function matchesPreparation(entry: Entry, target: Target): boolean {
  if (
    entry.transport !== getTransport() ||
    entry.target.agentType !== target.agentType
  ) {
    return false
  }
  if (entry.target.conversationId != null || target.conversationId != null) {
    return entry.target.conversationId === target.conversationId
  }
  return (
    (entry.target.sessionId ?? null) === (target.sessionId ?? null) &&
    (entry.target.workingDir ?? entry.reservedWorkingDir) === target.workingDir
  )
}

export async function awaitAcpPreparation(target: Target): Promise<void> {
  const matches = [...entries.values()].filter((entry) =>
    matchesPreparation(entry, target)
  )
  await Promise.all(matches.map((entry) => entry.promise))
}

export function consumeAcpPreparation(target: Target) {
  for (const entry of entries.values()) {
    if (matchesPreparation(entry, target)) {
      retire(entry)
    }
  }
}

export function scheduleAcpPreparation(target: Target): () => void {
  const timer = setTimeout(() => void prepareAcpSession(target), HOVER_DELAY_MS)
  return () => clearTimeout(timer)
}
