"use client"

import { useCallback, type ReactNode } from "react"
import type { PanInfo } from "motion/react"
import { Columns2, PanelRightClose, RotateCw, Rows2 } from "lucide-react"
import { useTabActions, useTabStore } from "@/contexts/tab-context"
import type { TabItem as TabItemData } from "@/contexts/tab-context"
import {
  clientPointFromDrag,
  dropIndexFromMidpoints,
} from "@/lib/tab-drag-drop"
import type { SplitDirection } from "@/lib/tab-group-layout"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"

interface DropTarget {
  groupId: string
  element: Element
  isStrip: boolean
}

function findDropTarget(
  sourceGroupId: string,
  clientX: number,
  clientY: number
): DropTarget | null {
  for (const element of document.elementsFromPoint(clientX, clientY)) {
    const strip = element.closest("[data-conv-group-strip]")
    const stripId = strip?.getAttribute("data-conv-group-strip")
    if (strip && stripId && stripId !== sourceGroupId) {
      return { groupId: stripId, element: strip, isStrip: true }
    }
    const shell = element.closest("[data-conv-group-shell]")
    const shellId = shell?.getAttribute("data-conv-group-shell")
    if (shell && shellId && shellId !== sourceGroupId) {
      return { groupId: shellId, element: shell, isStrip: false }
    }
  }
  return null
}

function dropIndex(target: DropTarget, clientX: number): number {
  if (!target.isStrip) return Number.MAX_SAFE_INTEGER
  const midpoints = Array.from(
    target.element.querySelectorAll("[data-tab-id]")
  ).map((element) => {
    const rect = element.getBoundingClientRect()
    return rect.left + rect.width / 2
  })
  return dropIndexFromMidpoints(clientX, midpoints)
}

export function useCrossGroupDrag(
  groupId: string | undefined,
  enabled: boolean
) {
  const { updateTabDrag, endTabDrag, moveTabToGroup } = useTabActions()
  const isDropTarget = useTabStore(
    (state) => groupId != null && state.tabDrag?.overGroupId === groupId
  )
  const onDrag = useCallback(
    (
      tab: TabItemData,
      event: MouseEvent | TouchEvent | PointerEvent,
      info: PanInfo
    ) => {
      if (!groupId || !enabled) return
      const point = clientPointFromDrag(event, info.point)
      const target = findDropTarget(groupId, point.x, point.y)
      updateTabDrag({
        tabId: tab.id,
        x: point.x,
        y: point.y,
        overGroupId: target?.groupId ?? null,
      })
    },
    [enabled, groupId, updateTabDrag]
  )
  const onDragEnd = useCallback(
    (
      tab: TabItemData,
      event: MouseEvent | TouchEvent | PointerEvent,
      info: PanInfo
    ) => {
      if (!groupId) return
      const point = clientPointFromDrag(event, info.point)
      const target = findDropTarget(groupId, point.x, point.y)
      endTabDrag()
      if (target) {
        moveTabToGroup(tab.id, target.groupId, dropIndex(target, point.x))
      }
    },
    [endTabDrag, groupId, moveTabToGroup]
  )
  return { isDropTarget, onDrag, onDragEnd }
}

function SplitAction({
  label,
  onClick,
  children,
}: {
  label: string
  onClick: () => void
  children: ReactNode
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          onClick={onClick}
          className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom">{label}</TooltipContent>
    </Tooltip>
  )
}

export function TabBarSplitControls({
  tabId,
  groupId,
  isSplit,
}: {
  tabId: string | null
  groupId: string
  isSplit: boolean
}) {
  const { splitTab, toggleGroupOrientation, dissolveGroup } = useTabActions()
  const split = useCallback(
    (direction: SplitDirection) => {
      if (tabId) splitTab(tabId, direction)
    },
    [splitTab, tabId]
  )
  return (
    <TooltipProvider delayDuration={300}>
      <div className="flex shrink-0 items-center gap-0.5 border-l border-border px-1">
        <SplitAction label="向右分屏" onClick={() => split("right")}>
          <Columns2 className="size-3.5" />
        </SplitAction>
        <SplitAction label="向下分屏" onClick={() => split("down")}>
          <Rows2 className="size-3.5" />
        </SplitAction>
        {isSplit && (
          <>
            <SplitAction
              label="切换分屏方向"
              onClick={() => toggleGroupOrientation(groupId)}
            >
              <RotateCw className="size-3.5" />
            </SplitAction>
            <SplitAction
              label="取消当前分屏"
              onClick={() => dissolveGroup(groupId)}
            >
              <PanelRightClose className="size-3.5" />
            </SplitAction>
          </>
        )}
      </div>
    </TooltipProvider>
  )
}
