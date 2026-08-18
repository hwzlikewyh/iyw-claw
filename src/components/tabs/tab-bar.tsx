"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import type { Dispatch, RefObject, SetStateAction, WheelEvent } from "react"
import { Reorder } from "motion/react"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { useTabActions, useTabStore } from "@/contexts/tab-context"
import type { TabItem as TabItemData } from "@/contexts/tab-context"
import { useWorkspaceView } from "@/contexts/workspace-context"
import { useIsCoarsePointer } from "@/hooks/use-is-coarse-pointer"
import { useShortcutSettings } from "@/hooks/use-shortcut-settings"
import { matchShortcutEvent } from "@/lib/keyboard-shortcuts"
import { groupOfTab } from "@/stores/tab-store"
import { firstLeafId, leafIds } from "@/lib/tab-group-layout"
import { TabItem } from "./tab-item"
import {
  TabBarSplitControls,
  useCrossGroupDrag,
} from "./tab-bar-split-controls"
import { cn } from "@/lib/utils"

interface TabBarProps {
  groupId?: string
}

type TabActions = ReturnType<typeof useTabActions>
type CrossGroupDrag = ReturnType<typeof useCrossGroupDrag>

function useTabGroupModel(groupId: string | undefined) {
  const tabs = useTabStore((state) => state.tabs)
  const activeTabId = useTabStore((state) => state.activeTabId)
  const groupOf = useTabStore((state) => state.groupOf)
  const groupLayout = useTabStore((state) => state.groupLayout)
  const groupSelection = useTabStore((state) => state.groupSelection)
  const tileByGroup = useTabStore((state) => state.tileByGroup)
  const stripGroupId = groupId ?? firstLeafId(groupLayout)
  const groupTabs = useMemo(
    () =>
      groupId == null
        ? tabs
        : tabs.filter(
            (tab) => groupOfTab(groupOf, groupLayout, tab.id) === groupId
          ),
    [groupId, groupLayout, groupOf, tabs]
  )
  const displayActiveId =
    groupId == null
      ? activeTabId
      : (groupSelection[groupId] ?? groupTabs[0]?.id ?? null)
  const isSplit = leafIds(groupLayout).length > 1
  const isTileMode = !!tileByGroup[stripGroupId]
  return {
    activeTabId,
    displayActiveId,
    groupTabs,
    isSplit,
    isTileMode,
    stripGroupId,
  }
}

function cycleTabId(
  tabs: TabItemData[],
  currentId: string,
  offset: number
): string | null {
  const current = tabs.findIndex((tab) => tab.id === currentId)
  if (current < 0) return null
  return tabs[(current + offset + tabs.length) % tabs.length]?.id ?? null
}

interface TabShortcutOptions {
  groupId?: string
  groupTabs: TabItemData[]
  displayActiveId: string | null
  activeTabId: string | null
  switchTab: TabActions["switchTab"]
  closeTab: TabActions["closeTab"]
}

function useTabBarShortcuts(options: TabShortcutOptions): void {
  const { mode, activePane, filesMaximized } = useWorkspaceView()
  const { shortcuts } = useShortcutSettings()
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const ownsShortcut =
        options.groupId == null ||
        options.displayActiveId === options.activeTabId
      const conversationVisible =
        mode === "conversation" ||
        (mode === "fusion" && activePane === "conversation" && !filesMaximized)
      if (!ownsShortcut || !conversationVisible) return
      const next = matchShortcutEvent(event, shortcuts.next_tab)
      const previous = matchShortcutEvent(event, shortcuts.prev_tab)
      if (next || previous) {
        if (options.groupTabs.length < 2 || !options.displayActiveId) return
        const target = cycleTabId(
          options.groupTabs,
          options.displayActiveId,
          next ? 1 : -1
        )
        if (!target) return
        event.preventDefault()
        options.switchTab(target)
        return
      }
      if (!matchShortcutEvent(event, shortcuts.close_current_tab)) return
      if (!options.activeTabId) return
      event.preventDefault()
      options.closeTab(options.activeTabId)
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [activePane, filesMaximized, mode, options, shortcuts])
}

function useTabStripScroll(displayActiveId: string | null) {
  const scrollRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!displayActiveId || !scrollRef.current) return
    scrollRef.current
      .querySelector(`[data-tab-id="${displayActiveId}"]`)
      ?.scrollIntoView({ block: "nearest", inline: "nearest" })
  }, [displayActiveId])
  const handleWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
    if (event.deltaY === 0 || !scrollRef.current) return
    event.preventDefault()
    scrollRef.current.scrollLeft += event.deltaY
  }, [])
  return { scrollRef, handleWheel }
}

function useTabReorder(groupId: string | undefined, actions: TabActions) {
  const isCoarsePointer = useIsCoarsePointer()
  const [touchSortingTabId, setTouchSortingTabId] = useState<string | null>(
    null
  )
  const handleReorder = useCallback(
    (nextTabs: TabItemData[]) => {
      if (isCoarsePointer && !touchSortingTabId) return
      if (groupId) actions.reorderGroupTabs(groupId, nextTabs)
      else actions.reorderTabs(nextTabs)
    },
    [actions, groupId, isCoarsePointer, touchSortingTabId]
  )
  return {
    handleReorder,
    isCoarsePointer,
    setTouchSortingTabId,
    touchSortingTabId,
  }
}

interface TabBarItemsProps {
  actions: TabActions
  branches: ReadonlyMap<number, string | null>
  drag: CrossGroupDrag
  folderIndex: ReadonlyMap<number, { name: string }>
  groupId?: string
  groupTabs: TabItemData[]
  displayActiveId: string | null
  isCoarsePointer: boolean
  isTileMode: boolean
  setTouchSortingTabId: Dispatch<SetStateAction<string | null>>
  stripGroupId: string
  touchSortingTabId: string | null
}

function TabBarItems(props: TabBarItemsProps) {
  return props.groupTabs.map((tab) => {
    const folder = props.folderIndex.get(tab.folderId)
    const crossGroupDrag = props.groupId != null && tab.conversationId != null
    return (
      <TabItem
        key={tab.id}
        tab={tab}
        isActive={tab.id === props.displayActiveId}
        isTileMode={props.isTileMode}
        folderName={folder?.name ?? null}
        folderBranch={props.branches.get(tab.folderId) ?? null}
        onSwitch={props.actions.switchTab}
        onClose={props.actions.closeTab}
        onCloseOthers={props.actions.closeOtherTabs}
        onCloseAll={props.actions.closeAllTabs}
        onPin={props.actions.pinTab}
        onToggleTile={() => props.actions.toggleGroupTile(props.stripGroupId)}
        isCoarsePointer={props.isCoarsePointer}
        isTouchSorting={props.touchSortingTabId === tab.id}
        onTouchSortingStart={props.setTouchSortingTabId}
        onTouchSortingEnd={() => props.setTouchSortingTabId(null)}
        onTabDrag={crossGroupDrag ? props.drag.onDrag : undefined}
        onTabDragEnd={crossGroupDrag ? props.drag.onDragEnd : undefined}
      />
    )
  })
}

interface TabBarStripContentProps extends TabBarItemsProps {
  handleReorder: (nextTabs: TabItemData[]) => void
  handleWheel: (event: WheelEvent<HTMLDivElement>) => void
  isDropTarget: boolean
  isHovered: boolean
  isSplit: boolean
  setIsHovered: Dispatch<SetStateAction<boolean>>
}

interface TabBarStripProps {
  scrollRef: RefObject<HTMLDivElement | null>
  content: TabBarStripContentProps
}

function TabBarStrip({ scrollRef, content }: TabBarStripProps) {
  return (
    <Reorder.Group
      as="div"
      ref={scrollRef}
      role="tablist"
      axis="x"
      values={content.groupTabs}
      onReorder={content.handleReorder}
      onWheel={content.handleWheel}
      onMouseEnter={() => content.setIsHovered(true)}
      onMouseLeave={() => content.setIsHovered(false)}
      data-conv-group-strip={content.groupId}
      className={cn(
        "flex h-10 items-stretch gap-1.5 overflow-x-scroll border-b border-border px-1.5 pt-1.5",
        content.isDropTarget && "bg-primary/8",
        content.isHovered
          ? "pb-0.5 [&::-webkit-scrollbar]:h-1 [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:bg-border"
          : "pb-1.5 [&::-webkit-scrollbar]:h-0"
      )}
    >
      <TabBarItems {...content} />
      <TabBarSplitControls
        tabId={content.displayActiveId}
        groupId={content.stripGroupId}
        isSplit={content.isSplit}
      />
    </Reorder.Group>
  )
}

export function TabBar({ groupId }: TabBarProps) {
  const model = useTabGroupModel(groupId)
  const actions = useTabActions()
  const allFolders = useAppWorkspaceStore((state) => state.allFolders)
  const branches = useAppWorkspaceStore((state) => state.branches)
  const folderIndex = useMemo(
    () => new Map(allFolders.map((folder) => [folder.id, folder])),
    [allFolders]
  )
  const scroll = useTabStripScroll(model.displayActiveId)
  const reorder = useTabReorder(groupId, actions)
  const drag = useCrossGroupDrag(groupId, model.isSplit)
  const [isHovered, setIsHovered] = useState(false)
  useTabBarShortcuts({ groupId, ...model, ...actions })
  if (model.groupTabs.length === 0) return null
  const stripContent: TabBarStripContentProps = {
    ...model,
    ...reorder,
    actions,
    branches,
    drag,
    folderIndex,
    groupId,
    handleWheel: scroll.handleWheel,
    isDropTarget: drag.isDropTarget,
    isHovered,
    setIsHovered,
  }
  return <TabBarStrip scrollRef={scroll.scrollRef} content={stripContent} />
}
