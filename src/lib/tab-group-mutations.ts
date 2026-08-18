import {
  firstLeafId,
  leafIds,
  makeGroupId,
  neighborGroupId,
  removeGroup,
  resizeSplitAt,
  singleGroupLayout,
  splitGroup,
  toggleOrientation,
  type LayoutNode,
  type SplitDirection,
} from "./tab-group-layout"
import { groupOfTab } from "./tab-group-state"

export interface MutableGroupTab {
  id: string
  conversationId: number | null
}

export interface MutableGroupState<T extends MutableGroupTab> {
  rawTabs: T[]
  activeTabId: string | null
  groupLayout: LayoutNode
  groupOf: Record<string, string>
  tileByGroup: Record<string, boolean>
  isTileMode: boolean
}

export interface SplitMutation {
  sourceGroup: string
  newGroupId: string
  groupLayout: LayoutNode
}

export function beginGroupSplit<T extends MutableGroupTab>(
  state: MutableGroupState<T>,
  tabId: string,
  direction: SplitDirection
): SplitMutation | null {
  if (!state.rawTabs.some((tab) => tab.id === tabId)) return null
  const sourceGroup = groupOfTab(state.groupOf, state.groupLayout, tabId)
  const newGroupId = makeGroupId()
  const groupLayout = splitGroup(state.groupLayout, sourceGroup, {
    direction,
    newGroupId,
  })
  if (groupLayout === state.groupLayout) return null
  return { sourceGroup, newGroupId, groupLayout }
}

export function toggleGroupTileState<T extends MutableGroupTab>(
  state: MutableGroupState<T>,
  groupId: string
): Pick<MutableGroupState<T>, "tileByGroup" | "isTileMode"> | null {
  if (!leafIds(state.groupLayout).includes(groupId)) return null
  const tileByGroup = {
    ...state.tileByGroup,
    [groupId]: !state.tileByGroup[groupId],
  }
  const activeGroup = state.activeTabId
    ? groupOfTab(state.groupOf, state.groupLayout, state.activeTabId)
    : firstLeafId(state.groupLayout)
  return {
    tileByGroup,
    isTileMode:
      activeGroup === groupId ? tileByGroup[groupId] : state.isTileMode,
  }
}

export function moveGroupTab<T extends MutableGroupTab>(
  state: MutableGroupState<T>,
  tabId: string,
  target: { groupId: string; index?: number }
): Pick<MutableGroupState<T>, "rawTabs" | "activeTabId" | "groupOf"> | null {
  const { groupId: targetGroupId, index } = target
  const moving = state.rawTabs.find((tab) => tab.id === tabId)
  if (!moving || moving.conversationId == null) return null
  if (!leafIds(state.groupLayout).includes(targetGroupId)) return null
  if (groupOfTab(state.groupOf, state.groupLayout, tabId) === targetGroupId) {
    return null
  }
  const rawTabs =
    index == null
      ? state.rawTabs
      : insertIntoGroup(state, moving, {
          groupId: targetGroupId,
          index,
        })
  return {
    rawTabs,
    activeTabId: tabId,
    groupOf: { ...state.groupOf, [tabId]: targetGroupId },
  }
}

function insertIntoGroup<T extends MutableGroupTab>(
  state: MutableGroupState<T>,
  moving: T,
  target: { groupId: string; index: number }
): T[] {
  const { groupId: targetGroupId, index } = target
  const without = state.rawTabs.filter((tab) => tab.id !== moving.id)
  const slots: number[] = []
  without.forEach((tab, slot) => {
    if (
      groupOfTab(state.groupOf, state.groupLayout, tab.id) === targetGroupId
    ) {
      slots.push(slot)
    }
  })
  const targetIndex = Math.max(0, Math.min(index, slots.length))
  const insertAt =
    targetIndex < slots.length
      ? slots[targetIndex]
      : slots.length > 0
        ? slots[slots.length - 1] + 1
        : without.length
  return [...without.slice(0, insertAt), moving, ...without.slice(insertAt)]
}

export function toggleGroupOrientationState<T extends MutableGroupTab>(
  state: MutableGroupState<T>,
  groupId: string
): LayoutNode | null {
  const layout = toggleOrientation(state.groupLayout, groupId)
  return layout === state.groupLayout ? null : layout
}

export function dissolveGroupState<T extends MutableGroupTab>(
  state: MutableGroupState<T>,
  groupId: string
): Pick<MutableGroupState<T>, "groupLayout" | "groupOf"> | null {
  const target = neighborGroupId(state.groupLayout, groupId)
  if (!target) return null
  const groupOf = { ...state.groupOf }
  for (const tab of state.rawTabs) {
    if (groupOfTab(state.groupOf, state.groupLayout, tab.id) === groupId) {
      groupOf[tab.id] = target
    }
  }
  return { groupOf, groupLayout: removeGroup(state.groupLayout, groupId) }
}

export function unsplitGroupState<T extends MutableGroupTab>(
  state: MutableGroupState<T>
): Pick<MutableGroupState<T>, "groupLayout" | "groupOf"> | null {
  if (state.groupLayout.type === "group") return null
  return {
    groupLayout: singleGroupLayout(firstLeafId(state.groupLayout)),
    groupOf: {},
  }
}

export function reorderGroupTabState<T extends MutableGroupTab>(
  state: MutableGroupState<T>,
  groupId: string,
  orderedTabs: MutableGroupTab[]
): T[] | null {
  const slots = state.rawTabs.flatMap((tab, index) =>
    groupOfTab(state.groupOf, state.groupLayout, tab.id) === groupId
      ? [index]
      : []
  )
  if (slots.length !== orderedTabs.length) return null
  const expected = new Set(slots.map((slot) => state.rawTabs[slot].id))
  const seen = new Set<string>()
  for (const tab of orderedTabs) {
    if (!expected.has(tab.id) || seen.has(tab.id)) return null
    seen.add(tab.id)
  }
  const byId = new Map(state.rawTabs.map((tab) => [tab.id, tab]))
  const next = [...state.rawTabs]
  slots.forEach((slot, index) => {
    const tab = byId.get(orderedTabs[index].id)
    if (tab) next[slot] = tab
  })
  return next.every((tab, index) => tab === state.rawTabs[index]) ? null : next
}

export function resizeGroupState<T extends MutableGroupTab>(
  state: MutableGroupState<T>,
  splitId: string,
  resize: { handleIndex: number; boundaryFraction: number }
): LayoutNode | null {
  const layout = resizeSplitAt(state.groupLayout, splitId, resize)
  return layout === state.groupLayout ? null : layout
}
