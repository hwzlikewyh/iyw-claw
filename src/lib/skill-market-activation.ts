import type { SkillMarketV2Detail } from "@/lib/skill-market"
import type {
  AgentSkillScope,
  AgentType,
  LogicalSkillInventoryItem,
  SkillInventorySnapshot,
} from "@/lib/types"

export type SkillMarketActivationKind =
  | "not_installed"
  | "loading"
  | "connector_only"
  | "unavailable"
  | "inactive"
  | "partial"
  | "active"

export interface SkillMarketActivationTarget {
  skillId: string
  scope: AgentSkillScope
  agentType: AgentType
  actualEnabled: boolean
  requiredBy: string[]
  blockedReasons: string[]
}

export interface SkillMarketActivationAgent {
  agentType: AgentType
  enabledCount: number
  totalCount: number
  required: boolean
  blocked: boolean
  requiredBy: string[]
  blockedReasons: string[]
}

export interface SkillMarketActivationSummary {
  kind: SkillMarketActivationKind
  targets: SkillMarketActivationTarget[]
  agents: SkillMarketActivationAgent[]
  enabledAgentCount: number
  agentCount: number
  enabledCount: number
  totalCount: number
  canEnable: boolean
  canDisable: boolean
}

const EMPTY_SUMMARY = {
  targets: [],
  agents: [],
  enabledAgentCount: 0,
  agentCount: 0,
  enabledCount: 0,
  totalCount: 0,
  canEnable: false,
  canDisable: false,
}

function inventoryMatchesMarketSkill(
  skill: LogicalSkillInventoryItem,
  marketSkillId: string
): boolean {
  return skill.observations.some(
    (observation) => observation.marketSkillId === marketSkillId
  )
}

function structuralBlockers(skill: LogicalSkillInventoryItem): string[] {
  const blockers: string[] = []
  if (skill.conflict) blockers.push("conflict")
  if (skill.duplicate) blockers.push("duplicate")
  if (skill.staleMarketRecord) blockers.push("stale_market_record")
  if (!skill.pluginAvailable) blockers.push("plugin_unavailable")
  if (skill.observations.every((observation) => observation.readOnly)) {
    blockers.push("read_only")
  }
  return blockers
}

function targetAgents(
  detail: SkillMarketV2Detail,
  skills: LogicalSkillInventoryItem[]
): AgentType[] {
  if (detail.installTargets.length) {
    return [...new Set(detail.installTargets)]
  }
  return [
    ...new Set(
      skills.flatMap((skill) =>
        skill.agentStates.map((state) => state.agentType)
      )
    ),
  ]
}

export function isConnectorOnlyPlugin(detail: SkillMarketV2Detail): boolean {
  const plugin = detail.currentVersion.plugin
  const components = plugin?.components ?? []
  return (
    detail.packageType === "plugin" &&
    plugin?.schemaVersion === 1 &&
    components.some((component) => component.type === "connector") &&
    !components.some((component) => component.type === "skill")
  )
}

function buildTargets(
  skills: LogicalSkillInventoryItem[],
  agentTypes: AgentType[]
): SkillMarketActivationTarget[] {
  return skills.flatMap((skill) => {
    const skillBlockers = structuralBlockers(skill)
    return agentTypes.map((agentType) => {
      const state = skill.agentStates.find(
        (candidate) => candidate.agentType === agentType
      )
      return {
        skillId: skill.skillId,
        scope: skill.scope,
        agentType,
        actualEnabled: state?.actualEnabled ?? false,
        requiredBy: state?.requiredBy ?? [],
        blockedReasons: [
          ...new Set([...skillBlockers, ...(state?.blockedReasons ?? [])]),
        ],
      }
    })
  })
}

function buildAgents(
  targets: SkillMarketActivationTarget[],
  agentTypes: AgentType[]
): SkillMarketActivationAgent[] {
  return agentTypes.map((agentType) => {
    const agentTargets = targets.filter(
      (target) => target.agentType === agentType
    )
    return {
      agentType,
      enabledCount: agentTargets.filter((target) => target.actualEnabled)
        .length,
      totalCount: agentTargets.length,
      required: agentTargets.some((target) => target.requiredBy.length > 0),
      blocked: agentTargets.every((target) => target.blockedReasons.length > 0),
      requiredBy: [
        ...new Set(agentTargets.flatMap((target) => target.requiredBy)),
      ],
      blockedReasons: [
        ...new Set(agentTargets.flatMap((target) => target.blockedReasons)),
      ],
    }
  })
}

export function buildSkillMarketActivation(
  detail: SkillMarketV2Detail | null,
  snapshot: SkillInventorySnapshot | null,
  loading: boolean
): SkillMarketActivationSummary {
  if (!detail || detail.installState === "not_installed") {
    return { kind: "not_installed", ...EMPTY_SUMMARY }
  }
  if (isConnectorOnlyPlugin(detail)) {
    return { kind: "connector_only", ...EMPTY_SUMMARY }
  }
  if (loading && !snapshot) return { kind: "loading", ...EMPTY_SUMMARY }
  if (!snapshot) return { kind: "unavailable", ...EMPTY_SUMMARY }

  const skills = snapshot.skills.filter((skill) =>
    inventoryMatchesMarketSkill(skill, detail.id)
  )
  const agentTypes = targetAgents(detail, skills)
  const targets = buildTargets(skills, agentTypes)
  if (!targets.length) return { kind: "unavailable", ...EMPTY_SUMMARY }

  const enabledCount = targets.filter((target) => target.actualEnabled).length
  const kind =
    enabledCount === 0
      ? "inactive"
      : enabledCount === targets.length
        ? "active"
        : "partial"
  const agents = buildAgents(targets, agentTypes)
  return {
    kind,
    targets,
    agents,
    enabledAgentCount: agents.filter(
      (agent) => agent.totalCount > 0 && agent.enabledCount === agent.totalCount
    ).length,
    agentCount: agents.length,
    enabledCount,
    totalCount: targets.length,
    canEnable: targets.some(
      (target) => !target.actualEnabled && !target.blockedReasons.length
    ),
    canDisable: targets.some(
      (target) =>
        target.actualEnabled &&
        !target.requiredBy.length &&
        !target.blockedReasons.length
    ),
  }
}
