import {
  firstLeafId,
  leafIds,
  normalizeTree,
  type LayoutNode,
} from "./tab-group-layout"
import type {
  PersistedGroupDraft,
  TabGroupSnapshot,
} from "./tab-group-persistence"

export interface GroupTabRecord {
  id: string
  folderId: number
  conversationId: number | null
  agentType: string
  isChat?: boolean
  workingDir?: string
  agentTypeProvisional?: boolean
}

export interface GroupStateRecord<T extends GroupTabRecord> {
  rawTabs: T[]
  activeTabId: string | null
  tabsHydrated: boolean
  isTileMode: boolean
  groupLayout: LayoutNode
  groupOf: Record<string, string>
  groupSelection: Record<string, string>
  tileByGroup: Record<string, boolean>
}

export type GroupStateProjection = Pick<
  GroupStateRecord<GroupTabRecord>,
  "groupLayout" | "groupOf" | "groupSelection" | "tileByGroup" | "isTileMode"
>

export function groupOfTab(
  assignments: Record<string, string>,
  layout: LayoutNode,
  tabId: string
): string {
  const assigned = assignments[tabId]
  return assigned && leafIds(layout).includes(assigned)
    ? assigned
    : firstLeafId(layout)
}

export function selectIsSplit(state: { groupLayout: LayoutNode }): boolean {
  return state.groupLayout.type === "split"
}

export function isReparentUnmount<T extends GroupTabRecord>(
  state: Pick<GroupStateRecord<T>, "rawTabs" | "groupOf" | "groupLayout">,
  tabId: string,
  renderedGroupId: string
): boolean {
  if (!state.rawTabs.some((tab) => tab.id === tabId)) return false
  return groupOfTab(state.groupOf, state.groupLayout, tabId) !== renderedGroupId
}

export function resolveTargetGroup(
  state: Pick<
    GroupStateRecord<GroupTabRecord>,
    "activeTabId" | "groupOf" | "groupLayout"
  >,
  explicit?: string
): string {
  if (explicit && leafIds(state.groupLayout).includes(explicit)) return explicit
  return state.activeTabId
    ? groupOfTab(state.groupOf, state.groupLayout, state.activeTabId)
    : firstLeafId(state.groupLayout)
}

export function restorePersistedDrafts<T extends GroupTabRecord>(
  restored: T[],
  drafts: PersistedGroupDraft[],
  materialize: (draft: PersistedGroupDraft) => T
): { tabs: T[]; assignments: Record<string, string> } {
  const tabs = [...restored]
  const assignments: Record<string, string> = {}
  const seen = new Set(tabs.map((tab) => tab.id))
  for (const draft of [...drafts].sort((a, b) => a.index - b.index)) {
    if (seen.has(draft.id)) continue
    seen.add(draft.id)
    tabs.splice(Math.min(draft.index, tabs.length), 0, materialize(draft))
    assignments[draft.id] = draft.group
  }
  return { tabs, assignments }
}

export function buildTabGroupSnapshot<T extends GroupTabRecord>(
  state: GroupStateRecord<T>,
  syncKey: (tab: T) => string | null
): TabGroupSnapshot {
  const assignments: Record<string, string> = {}
  const drafts: PersistedGroupDraft[] = []
  state.rawTabs.forEach((tab, index) => {
    const group = groupOfTab(state.groupOf, state.groupLayout, tab.id)
    const key = syncKey(tab)
    if (key) {
      assignments[key] = group
      return
    }
    drafts.push({
      id: tab.id,
      group,
      index,
      folderId: tab.folderId,
      ...(tab.isChat ? { isChat: true } : {}),
      ...(tab.isChat || !tab.workingDir ? {} : { workingDir: tab.workingDir }),
      ...(tab.agentTypeProvisional ? {} : { agentType: tab.agentType }),
    })
  })
  const selection = buildPersistedSelection(state, syncKey)
  const active = state.rawTabs.find((tab) => tab.id === state.activeTabId)
  return {
    layout: state.groupLayout,
    assignments,
    selection,
    tileByGroup: state.tileByGroup,
    drafts,
    activeDraft: active?.conversationId == null ? (active?.id ?? null) : null,
  }
}

function buildPersistedSelection<T extends GroupTabRecord>(
  state: GroupStateRecord<T>,
  syncKey: (tab: T) => string | null
): Record<string, string> {
  const selection: Record<string, string> = {}
  for (const [groupId, tabId] of Object.entries(state.groupSelection)) {
    const tab = state.rawTabs.find((item) => item.id === tabId)
    if (tab) selection[groupId] = syncKey(tab) ?? tab.id
  }
  return selection
}

function sameRecord<T extends string | boolean>(
  left: Record<string, T>,
  right: Record<string, T>
): boolean {
  const keys = Object.keys(left)
  return (
    keys.length === Object.keys(right).length &&
    keys.every((key) => left[key] === right[key])
  )
}

function pruneProjection<T extends GroupTabRecord>(
  state: GroupStateRecord<T>,
  tabSetKnown: boolean
): { layout: LayoutNode; assignments: Record<string, string> } {
  if (!tabSetKnown) {
    return { layout: state.groupLayout, assignments: state.groupOf }
  }
  const openIds = new Set(state.rawTabs.map((tab) => tab.id))
  const leaves = new Set(leafIds(state.groupLayout))
  const pruned = Object.fromEntries(
    Object.entries(state.groupOf).filter(
      ([tabId, groupId]) => openIds.has(tabId) && leaves.has(groupId)
    )
  )
  const assignments = sameRecord(pruned, state.groupOf) ? state.groupOf : pruned
  const liveGroups = new Set(
    state.rawTabs.map((tab) =>
      groupOfTab(assignments, state.groupLayout, tab.id)
    )
  )
  const layout = normalizeTree(state.groupLayout, liveGroups)
  const nextLeaves = new Set(leafIds(layout))
  const revalidated = Object.fromEntries(
    Object.entries(assignments).filter(([, groupId]) => nextLeaves.has(groupId))
  )
  return {
    layout,
    assignments: sameRecord(revalidated, assignments)
      ? assignments
      : revalidated,
  }
}

function buildSelection<T extends GroupTabRecord>(options: {
  state: GroupStateRecord<T>
  layout: LayoutNode
  assignments: Record<string, string>
  tabSetKnown: boolean
}): Record<string, string> {
  const { state, layout, assignments, tabSetKnown } = options
  const selection: Record<string, string> = {}
  const active = state.rawTabs.some((tab) => tab.id === state.activeTabId)
    ? state.activeTabId
    : null
  const activeGroup = active ? groupOfTab(assignments, layout, active) : null
  for (const groupId of leafIds(layout)) {
    const members = state.rawTabs.filter(
      (tab) => groupOfTab(assignments, layout, tab.id) === groupId
    )
    if (members.length === 0) {
      const current = state.groupSelection[groupId]
      if (!tabSetKnown && current) selection[groupId] = current
      continue
    }
    const current = state.groupSelection[groupId]
    selection[groupId] =
      activeGroup === groupId && active
        ? active
        : (members.find((tab) => tab.id === current)?.id ?? members[0].id)
  }
  return selection
}

export function reconcileTabGroupState<T extends GroupTabRecord>(
  state: GroupStateRecord<T>,
  tabSetKnown: boolean
): GroupStateProjection | null {
  const projection = pruneProjection(state, tabSetKnown)
  const selection = buildSelection({
    state,
    layout: projection.layout,
    assignments: projection.assignments,
    tabSetKnown,
  })
  const leaves = new Set(leafIds(projection.layout))
  const tiles = tabSetKnown
    ? Object.fromEntries(
        Object.entries(state.tileByGroup).filter(([groupId]) =>
          leaves.has(groupId)
        )
      )
    : state.tileByGroup
  const activeGroup = state.activeTabId
    ? groupOfTab(projection.assignments, projection.layout, state.activeTabId)
    : firstLeafId(projection.layout)
  const groupSelection = sameRecord(selection, state.groupSelection)
    ? state.groupSelection
    : selection
  const tileByGroup = sameRecord(tiles, state.tileByGroup)
    ? state.tileByGroup
    : tiles
  const isTileMode = !!tileByGroup[activeGroup]
  if (
    projection.layout === state.groupLayout &&
    projection.assignments === state.groupOf &&
    groupSelection === state.groupSelection &&
    tileByGroup === state.tileByGroup &&
    isTileMode === state.isTileMode
  ) {
    return null
  }
  return {
    groupLayout: projection.layout,
    groupOf: projection.assignments,
    groupSelection,
    tileByGroup,
    isTileMode,
  }
}
