import type {
  AgentType,
  ManagedSkillFamilyState,
  ManagedSkillState,
  ManagedSkillSyncReport,
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
  skillStates: ManagedSkillState[]
  globalEnabled: boolean
  setGlobalEnabled: (enabled: boolean) => Promise<ManagedSkillSyncReport>
  setSkillEnabled: (
    skillId: string,
    enabled: boolean
  ) => Promise<ManagedSkillSyncReport>
  categoryOrder: Record<string, number>
  translateCategory: (category: string) => string
  loadContent?: (skillId: string) => Promise<string>
  onApplied?: (touchedAgents: AgentType[]) => void
  searchPlaceholder?: string
  notReadyHint?: string
}

export function stripFrontmatter(content: string): string {
  const match = content.match(/^---\s*\r?\n[\s\S]*?\r?\n---\s*(?:\r?\n)?/)
  return match ? content.slice(match[0].length) : content
}

export function mergeManagedSkillEnabled(
  state: ManagedSkillFamilyState | null,
  skillId: string,
  enabled: boolean
): ManagedSkillFamilyState | null {
  if (!state) return null
  const skills = state.skills.map((skill) =>
    skill.skillId === skillId ? { ...skill, enabled } : skill
  )
  return {
    ...state,
    allEnabled: skills.length > 0 && skills.every((skill) => skill.enabled),
    skills,
  }
}

export function mergeAllManagedSkillsEnabled(
  state: ManagedSkillFamilyState | null,
  enabled: boolean
): ManagedSkillFamilyState | null {
  if (!state) return null
  return {
    ...state,
    allEnabled: enabled,
    skills: state.skills.map((skill) => ({ ...skill, enabled })),
  }
}
