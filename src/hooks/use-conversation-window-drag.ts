import { useCallback } from "react"
import type { PanInfo } from "motion/react"
import type { TabItem } from "@/stores/tab-store"
import { useTabActions } from "@/stores/tab-store"
import { isDesktop } from "@/lib/platform"
import { clientPointFromDrag } from "@/lib/tab-drag-drop"
import { requestConversationWindow } from "@/lib/conversation-window-state"

const DETACH_DISTANCE_PX = 24

function leftTabStrip(tabId: string, point: { x: number; y: number }) {
  const item = document.querySelector(`[data-tab-id="${CSS.escape(tabId)}"]`)
  const strip = item?.closest("[data-conversation-drop-zone]")
  if (!strip) return false
  const sourceGroup = strip
    .querySelector("[data-conv-group-strip]")
    ?.getAttribute("data-conv-group-strip")
  const anotherGroup = document
    .elementsFromPoint(point.x, point.y)
    .some((element) => {
      const group = element
        .closest("[data-conv-group-shell]")
        ?.getAttribute("data-conv-group-shell")
      return group && sourceGroup && group !== sourceGroup
    })
  if (anotherGroup) return false
  const rect = strip.getBoundingClientRect()
  return (
    point.y < rect.top - DETACH_DISTANCE_PX ||
    point.y > rect.bottom + DETACH_DISTANCE_PX
  )
}

type DragEnd = (
  tab: TabItem,
  event: MouseEvent | TouchEvent | PointerEvent,
  info: PanInfo
) => void

export function useConversationWindowDrag(onGroupDragEnd: DragEnd) {
  const { endTabDrag } = useTabActions()
  return useCallback<DragEnd>(
    (tab, event, info) => {
      if (event.type.includes("cancel")) {
        endTabDrag()
        return
      }
      const point = clientPointFromDrag(event, info.point)
      const outside =
        point.x < 0 ||
        point.y < 0 ||
        point.x >= window.innerWidth ||
        point.y >= window.innerHeight
      const isTouch =
        "touches" in event ||
        ("pointerType" in event && event.pointerType === "touch")
      if (isDesktop() && (outside || leftTabStrip(tab.id, point)) && !isTouch) {
        endTabDrag()
        requestConversationWindow(tab.id)
        return
      }
      onGroupDragEnd(tab, event, info)
    },
    [endTabDrag, onGroupDragEnd]
  )
}
