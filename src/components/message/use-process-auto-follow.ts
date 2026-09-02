import {
  useCallback,
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

  const scrollToLatest = useCallback(() => {
    const viewport = viewportRef.current
    if (!viewport) return
    viewport.scrollTop = viewport.scrollHeight
    setFollowing(true)
  }, [setFollowing])

  const handleScroll = useCallback<UIEventHandler<HTMLDivElement>>(
    (event) => setFollowing(isNearScrollEnd(event.currentTarget)),
    [setFollowing]
  )

  useLayoutEffect(() => {
    if (!active || !followingRef.current) return
    const frame = requestAnimationFrame(scrollToLatest)
    return () => cancelAnimationFrame(frame)
  }, [active, contentVersion, scrollToLatest])

  return { handleScroll, isFollowing, scrollToLatest, viewportRef }
}
