import type {
  AcpAgentInfo,
  AgentType,
  ExpertInstallStatus,
  LinkOp,
  LinkOpResult,
} from "@/lib/types"

export interface SkillToggleItem {
  id: string
  category: string
  displayName: string
  description: string
  ready: boolean
  badge?: { label: string; tone: "amber" | "muted" }
}

export interface SkillToggleListProps {
  skills: SkillToggleItem[]
  agents: AcpAgentInfo[]
  categoryOrder: Record<string, number>
  translateCategory: (category: string) => string
  loadAllStatuses: () => Promise<ExpertInstallStatus[]>
  applyLinks: (ops: LinkOp[]) => Promise<LinkOpResult[]>
  loadContent?: (skillId: string) => Promise<string>
  onApplied?: (touchedAgents: AgentType[]) => void
  statusReloadToken?: number
  searchPlaceholder?: string
  notReadyHint?: string
}

export function statusKey(skillId: string, agentType: AgentType): string {
  return `${skillId}:${agentType}`
}

export function isEnabled(status: ExpertInstallStatus | undefined): boolean {
  return status?.state === "linked_to_iyw_claw"
}

export function isBlocked(status: ExpertInstallStatus | undefined): boolean {
  return (
    status?.state === "blocked_by_real_directory" ||
    status?.state === "linked_elsewhere"
  )
}

export function buildStatusMap(statuses: ExpertInstallStatus[]) {
  return new Map(
    statuses.map((status) => [
      statusKey(status.expertId, status.agentType),
      status,
    ])
  )
}

export function stripFrontmatter(content: string): string {
  const match = content.match(/^---\s*\r?\n[\s\S]*?\r?\n---\s*(?:\r?\n)?/)
  return match ? content.slice(match[0].length) : content
}
