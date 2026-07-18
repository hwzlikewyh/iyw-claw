import { useInsertionEffect, useLayoutEffect, useRef } from "react"

export const PANEL_SLIDE_DURATION_MS = 180
export const PANEL_SLIDE_FALLBACK_MS = 320
const PANEL_SLIDE_CLASS_NAME = "panel-slide-animating"

export function usePanelSlideOnToggle(
  groupId: string,
  stateKey: string | number | boolean,
  ready: boolean
): void {
  const previousStateRef = useRef(stateKey)
  const previousReadyRef = useRef(ready)
  const timerRef = useRef<number | null>(null)
  const activeGroupRef = useRef<HTMLElement | null>(null)
  const transitionEndRef = useRef<EventListener | null>(null)

  useInsertionEffect(() => {
    const shouldAnimate =
      ready && previousReadyRef.current && previousStateRef.current !== stateKey

    previousStateRef.current = stateKey
    previousReadyRef.current = ready

    if (!shouldAnimate) return

    const group = document.querySelector<HTMLElement>(
      `[data-panel-group-id="${groupId}"]`
    )
    if (!group) return

    const previousGroup = activeGroupRef.current
    const previousListener = transitionEndRef.current
    if (previousGroup && previousListener) {
      previousGroup.removeEventListener("transitionend", previousListener)
    }
    if (previousGroup && previousGroup !== group) {
      previousGroup.classList.remove(PANEL_SLIDE_CLASS_NAME)
    }
    if (timerRef.current !== null) window.clearTimeout(timerRef.current)

    const finish = () => {
      group.classList.remove(PANEL_SLIDE_CLASS_NAME)
      group.removeEventListener("transitionend", handleTransitionEnd)
      if (timerRef.current !== null) window.clearTimeout(timerRef.current)
      timerRef.current = null
      activeGroupRef.current = null
      transitionEndRef.current = null
    }
    const handleTransitionEnd: EventListener = (event) => {
      const transitionEvent = event as TransitionEvent
      const target = event.target
      if (
        transitionEvent.propertyName !== "flex-grow" ||
        !(target instanceof HTMLElement) ||
        target.parentElement !== group ||
        !target.hasAttribute("data-panel")
      ) {
        return
      }
      finish()
    }

    group.classList.add(PANEL_SLIDE_CLASS_NAME)
    activeGroupRef.current = group
    transitionEndRef.current = handleTransitionEnd
    group.addEventListener("transitionend", handleTransitionEnd)
    timerRef.current = window.setTimeout(finish, PANEL_SLIDE_FALLBACK_MS)
  }, [groupId, ready, stateKey])

  useLayoutEffect(
    () => () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current)
      if (activeGroupRef.current && transitionEndRef.current) {
        activeGroupRef.current.removeEventListener(
          "transitionend",
          transitionEndRef.current
        )
      }
      activeGroupRef.current?.classList.remove(PANEL_SLIDE_CLASS_NAME)
    },
    []
  )
}
