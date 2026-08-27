"use client"

import { useState } from "react"
import { ProcessGroupSection } from "@/components/settings/performance-process-group-section"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import { isAgentType } from "@/lib/types"

export interface AppProcessInfo {
  pid: number
  displayName: string
  agentType: string | null
  isMainProcess: boolean
  cpuUsage: number
  memoryBytes: number
  privateMemoryBytes?: number
  groupId?: string
  groupDisplayName?: string
  processRole?: string
}

export interface AppAgentSessionInfo {
  connectionId: string
  conversationId?: number
  conversationTitle?: string
  agentType: string
  status: string
  launcherPid?: number
  lastActivityAt: string
  privateMemoryBytes?: number
  memoryBytes: number
  processCount: number
  recoverable: boolean
  protectionReason?: string
  canEnd: boolean
}

export interface ProcessGroup {
  id: string
  displayName: string
  rootPid: number
  cpuUsage: number
  memoryBytes: number
  privateMemoryBytes?: number
  isAgentSession: boolean
  session?: AppAgentSessionInfo
  processes: AppProcessInfo[]
}

function fallbackGroup(
  proc: AppProcessInfo
): Pick<ProcessGroup, "id" | "displayName"> {
  if (proc.isMainProcess) return { id: "main", displayName: "iyw-claw" }
  if (proc.agentType) {
    return {
      id: `legacy-agent-${proc.pid}`,
      displayName: `${proc.displayName}会话`,
    }
  }
  return { id: "other", displayName: "其他附属进程" }
}

function agentDisplayName(agentType: string): string {
  return isAgentType(agentType) ? getAgentDisplayName(agentType) : agentType
}

function groupRank(group: ProcessGroup): number {
  if (group.id === "main") return 0
  if (group.id.startsWith("webview2-")) return 1
  if (group.id.startsWith("managed-browser-")) return 2
  if (group.isAgentSession) return 3
  return 4
}

export function buildProcessGroups(
  processes: AppProcessInfo[],
  sessions: AppAgentSessionInfo[] = []
): ProcessGroup[] {
  const sessionsByGroup = new Map(
    sessions.map((session) => [`connection-${session.connectionId}`, session])
  )
  const groups = new Map<string, ProcessGroup>()
  for (const proc of processes) {
    const fallback = fallbackGroup(proc)
    const id = proc.groupId ?? fallback.id
    const existing = groups.get(id) ?? {
      id,
      displayName: proc.agentType
        ? agentDisplayName(proc.agentType)
        : (proc.groupDisplayName ?? fallback.displayName),
      rootPid: proc.pid,
      cpuUsage: 0,
      memoryBytes: 0,
      privateMemoryBytes: 0,
      isAgentSession: Boolean(proc.agentType),
      session: sessionsByGroup.get(id),
      processes: [],
    }
    existing.cpuUsage += proc.cpuUsage
    existing.memoryBytes += proc.memoryBytes
    if (
      proc.privateMemoryBytes != null &&
      existing.privateMemoryBytes != null
    ) {
      existing.privateMemoryBytes =
        existing.privateMemoryBytes + proc.privateMemoryBytes
    } else {
      existing.privateMemoryBytes = undefined
    }
    if (proc.agentType) existing.isAgentSession = true
    existing.processes.push(proc)
    if (
      proc.processRole === "main" ||
      proc.processRole === "launcher" ||
      proc.processRole === "controller"
    ) {
      existing.rootPid = proc.pid
    }
    groups.set(id, existing)
  }
  for (const session of sessions) {
    const id = `connection-${session.connectionId}`
    if (groups.has(id)) continue
    groups.set(id, {
      id,
      displayName: agentDisplayName(session.agentType),
      rootPid: session.launcherPid ?? 0,
      cpuUsage: 0,
      memoryBytes: session.memoryBytes,
      privateMemoryBytes: session.privateMemoryBytes,
      isAgentSession: true,
      session,
      processes: [],
    })
  }
  return Array.from(groups.values()).sort(
    (left, right) =>
      groupRank(left) - groupRank(right) ||
      (right.privateMemoryBytes ?? right.memoryBytes) -
        (left.privateMemoryBytes ?? left.memoryBytes) ||
      left.rootPid - right.rootPid
  )
}

export function ProcessGroupList({
  groups,
  endingConnectionIds,
  onEndSession,
}: {
  groups: ProcessGroup[]
  endingConnectionIds?: ReadonlySet<string>
  onEndSession?: (connectionId: string) => void
}) {
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set())
  const toggle = (groupId: string) => {
    setExpandedIds((current) => {
      const next = new Set(current)
      if (next.has(groupId)) next.delete(groupId)
      else next.add(groupId)
      return next
    })
  }

  return (
    <div>
      <div className="grid grid-cols-[minmax(0,1fr)_6rem_6rem_4rem] gap-2 border-b bg-muted/10 px-4 py-2 text-xs text-muted-foreground">
        <span>进程组</span>
        <span className="text-right">私有内存</span>
        <span className="text-right">工作集</span>
        <span className="text-right">CPU</span>
      </div>
      {groups.map((group) => (
        <ProcessGroupSection
          key={group.id}
          group={group}
          expanded={expandedIds.has(group.id)}
          onToggle={() => toggle(group.id)}
          ending={Boolean(
            group.session &&
            endingConnectionIds?.has(group.session.connectionId)
          )}
          onEndSession={onEndSession}
        />
      ))}
    </div>
  )
}
