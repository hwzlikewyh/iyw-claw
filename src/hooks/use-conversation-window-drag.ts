import { useCallback } from "react"
import type { PanInfo } from "motion/react"
import type { TabItem } from "@/stores/tab-store"
import { useTabActions } from "@/stores/tab-store"
import { isDesktop } from "@/lib/platform"
import { clientPointFromDrag } from "@/lib/tab-drag-drop"
import { requestConversationWindow } from "@/lib/conversation-window-state"

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
      if (isDesktop() && outside && !isTouch) {
        endTabDrag()
        requestConversationWindow(tab.id)
        return
      }
      onGroupDragEnd(tab, event, info)
    },
    [endTabDrag, onGroupDragEnd]
  )
}
