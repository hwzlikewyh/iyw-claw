import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type UIEventHandler,
} from "react"

const SCROLL_END_THRESHOLD = 24

export function isNearScrollEnd(element: HTMLElement): boolean {
  const distance =
    element.scrollHeight - element.clientHeight - element.scrollTop
  return distance <= SCROLL_END_THRESHOLD
}

export function useProcessAutoFollow(contentVersion: unknown, active: boolean) {
  const viewportRef = useRef<HTMLDivElement>(null)
  const followingRef = useRef(true)
  const [isFollowing, setIsFollowing] = useState(true)

  const setFollowing = useCallback((following: boolean) => {
    followingRef.current = following
    setIsFollowing(following)
  }, [])

  const alignToLatest = useCallback(() => {
    const viewport = viewportRef.current
    if (!viewport) return
    viewport.scrollTop = viewport.scrollHeight
  }, [])

  const scrollToLatest = useCallback(() => {
    alignToLatest()
    setFollowing(true)
  }, [alignToLatest, setFollowing])

  const handleScroll = useCallback<UIEventHandler<HTMLDivElement>>(
    (event) => setFollowing(isNearScrollEnd(event.currentTarget)),
    [setFollowing]
  )

  useLayoutEffect(() => {
    if (!active || !followingRef.current) return
    alignToLatest()
    const frame = requestAnimationFrame(scrollToLatest)
    return () => cancelAnimationFrame(frame)
  }, [active, alignToLatest, contentVersion, scrollToLatest])

  useEffect(() => {
    const viewport = viewportRef.current
    const content = viewport?.firstElementChild
    if (
      !active ||
      !viewport ||
      !content ||
      typeof ResizeObserver === "undefined"
    ) {
      return
    }
    let frame = 0
    const observer = new ResizeObserver(() => {
      if (!followingRef.current) return
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(scrollToLatest)
    })
    observer.observe(content)
    observer.observe(viewport)
    return () => {
      cancelAnimationFrame(frame)
      observer.disconnect()
    }
  }, [active, scrollToLatest])

  return { handleScroll, isFollowing, scrollToLatest, viewportRef }
}
