"use client"

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react"
import type { PanInfo } from "motion/react"
import {
  Columns2,
  Ellipsis,
  PanelRightClose,
  RotateCw,
  Rows2,
} from "lucide-react"
import { useTabActions, useTabStore } from "@/contexts/tab-context"
import type { TabItem as TabItemData } from "@/contexts/tab-context"
import {
  clientPointFromDrag,
  dropIndexFromMidpoints,
} from "@/lib/tab-drag-drop"
import type { SplitDirection } from "@/lib/tab-group-layout"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
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

const SPLIT_BUTTON_CLASS_NAME =
  "flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"

interface SplitControlAction {
  label: string
  onSelect: () => void
  icon: ReactNode
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
          className={SPLIT_BUTTON_CLASS_NAME}
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom">{label}</TooltipContent>
    </Tooltip>
  )
}

function CompactSplitControls({ actions }: { actions: SplitControlAction[] }) {
  const [open, setOpen] = useState(false)
  const triggerRef = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    const trigger = triggerRef.current
    if (!open || !trigger) return
    const observer = new ResizeObserver(() => {
      if (trigger.offsetWidth === 0) setOpen(false)
    })
    observer.observe(trigger)
    return () => observer.disconnect()
  }, [open])
  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              ref={triggerRef}
              type="button"
              aria-label="分屏操作"
              className={SPLIT_BUTTON_CLASS_NAME}
            >
              <Ellipsis className="size-3.5" />
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="bottom">分屏操作</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="end">
        {actions.map((action) => (
          <DropdownMenuItem key={action.label} onSelect={action.onSelect}>
            {action.icon}
            {action.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
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
  const actions: SplitControlAction[] = [
    {
      label: "向右分屏",
      onSelect: () => split("right"),
      icon: <Columns2 className="size-3.5" />,
    },
    {
      label: "向下分屏",
      onSelect: () => split("down"),
      icon: <Rows2 className="size-3.5" />,
    },
  ]
  if (isSplit) {
    actions.push(
      {
        label: "切换分屏方向",
        onSelect: () => toggleGroupOrientation(groupId),
        icon: <RotateCw className="size-3.5" />,
      },
      {
        label: "取消当前分屏",
        onSelect: () => dissolveGroup(groupId),
        icon: <PanelRightClose className="size-3.5" />,
      }
    )
  }
  return <SplitControls actions={actions} />
}

function SplitControls({ actions }: { actions: SplitControlAction[] }) {
  return (
    <TooltipProvider delayDuration={300}>
      <div className="h-7 shrink-0 border-l border-border px-1">
        <div className="hidden items-center gap-0.5 @min-[240px]/tab-strip:flex">
          {actions.map((action) => (
            <SplitAction
              key={action.label}
              label={action.label}
              onClick={action.onSelect}
            >
              {action.icon}
            </SplitAction>
          ))}
        </div>
        <div className="@min-[240px]/tab-strip:hidden">
          <CompactSplitControls actions={actions} />
        </div>
      </div>
    </TooltipProvider>
  )
}
