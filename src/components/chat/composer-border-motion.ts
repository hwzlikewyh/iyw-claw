import { useEffect, type RefObject } from "react"

const CORNER_RADIUS = 12
const CORNER_STEPS = 8
const BORDER_INSET = 1
const PIXELS_PER_SECOND = 240
const MILLISECONDS_PER_SECOND = 1_000

function motionFrame(
  position: [number, number],
  angle: number,
  offset: number
) {
  const [x, y] = position
  return {
    offset,
    transform: `translate3d(${x + BORDER_INSET}px, ${y + BORDER_INSET}px, 0) rotate(${angle}deg)`,
  }
}

function borderFrames(width: number, height: number) {
  const radius = Math.min(CORNER_RADIUS, width / 2, height / 2)
  const horizontal = width - 2 * radius
  const vertical = height - 2 * radius
  const arc = (Math.PI * radius) / 2
  const perimeter = 2 * (horizontal + vertical) + 4 * arc
  const corners = [
    { x: width - radius, y: radius, angle: -90, straight: horizontal },
    { x: width - radius, y: height - radius, angle: 0, straight: vertical },
    { x: radius, y: height - radius, angle: 90, straight: horizontal },
    { x: radius, y: radius, angle: 180, straight: vertical },
  ]
  const frames = [motionFrame([radius, 0], 0, 0)]
  let distance = 0
  for (const corner of corners) {
    distance += corner.straight
    for (let step = 0; step <= CORNER_STEPS; step += 1) {
      const degrees = corner.angle + (90 * step) / CORNER_STEPS
      const radians = (degrees * Math.PI) / 180
      frames.push(
        motionFrame(
          [
            corner.x + radius * Math.cos(radians),
            corner.y + radius * Math.sin(radians),
          ],
          degrees + 90,
          (distance + (arc * step) / CORNER_STEPS) / perimeter
        )
      )
    }
    distance += arc
  }
  frames[frames.length - 1] = motionFrame([radius, 0], 360, 1)
  return {
    frames,
    duration: (perimeter / PIXELS_PER_SECOND) * MILLISECONDS_PER_SECOND,
  }
}

export function useComposerBorderMotion(
  ref: RefObject<HTMLDivElement | null>,
  enabled: boolean
) {
  useEffect(() => {
    const root = ref.current
    const marker = root?.firstElementChild
    if (!enabled || !root || !(marker instanceof HTMLElement)) return
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)")
    let animation: Animation | undefined
    let duration = 0
    const syncPlayback = () => {
      if (document.hidden || reduced.matches) animation?.pause()
      else animation?.play()
    }
    const resize = () => {
      const width = root.clientWidth - BORDER_INSET * 2
      const height = root.clientHeight - BORDER_INSET * 2
      if (width <= 0 || height <= 0) return
      const progress = duration
        ? Number(animation?.currentTime ?? 0) / duration
        : 0
      const next = borderFrames(width, height)
      animation?.cancel()
      duration = next.duration
      animation = marker.animate(next.frames, {
        duration,
        iterations: Infinity,
        easing: "linear",
      })
      animation.currentTime = progress * duration
      syncPlayback()
    }
    // 只在尺寸变化时更新轨迹；逐帧运动交由浏览器合成，不跟随回复计时重建。
    const observer = new ResizeObserver(resize)
    observer.observe(root)
    document.addEventListener("visibilitychange", syncPlayback)
    reduced.addEventListener("change", syncPlayback)
    return () => {
      observer.disconnect()
      animation?.cancel()
      document.removeEventListener("visibilitychange", syncPlayback)
      reduced.removeEventListener("change", syncPlayback)
    }
  }, [ref, enabled])
}
