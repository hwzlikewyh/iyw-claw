import { useEffect } from "react"
import { getShellTransport, isDesktop } from "@/lib/transport"
import { isDetachedConversationWindow } from "@/lib/conversation-window-state"

const DROP_SELECTOR = "[data-conversation-drop-zone]"

export function useConversationDropBounds() {
  useEffect(() => {
    if (!isDesktop() || isDetachedConversationWindow()) return
    let frame = 0
    let lastBounds = ""
    const publish = () => {
      frame = 0
      const bounds = Array.from(document.querySelectorAll(DROP_SELECTOR))
        .map((element) => {
          observer.observe(element)
          const { left, top, width, height } = element.getBoundingClientRect()
          return { left, top, width, height }
        })
        .filter((rect) => rect.width > 0 && rect.height > 0)
      const serialized = JSON.stringify(bounds)
      if (serialized === lastBounds) return
      lastBounds = serialized
      void getShellTransport()
        .call("set_conversation_drop_bounds", { bounds })
        .catch((error) =>
          console.warn("[conversation-window] drop bounds update failed", error)
        )
    }
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(publish)
    }
    const observer = new ResizeObserver(schedule)
    const mutations = new MutationObserver((records) => {
      if (
        records.some((record) =>
          [...record.addedNodes, ...record.removedNodes].some(
            (node) =>
              node instanceof Element &&
              (node.matches(DROP_SELECTOR) || node.querySelector(DROP_SELECTOR))
          )
        )
      )
        schedule()
    })
    mutations.observe(document.body, { childList: true, subtree: true })
    window.addEventListener("resize", schedule)
    schedule()
    return () => {
      cancelAnimationFrame(frame)
      observer.disconnect()
      mutations.disconnect()
      window.removeEventListener("resize", schedule)
    }
  }, [])
}
