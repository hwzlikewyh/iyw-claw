"use client"

import { useEffect, useRef } from "react"

export function useMediaSource(src: string, visible: boolean, rate: number) {
  const ref = useRef<HTMLVideoElement & HTMLAudioElement>(null)
  const position = useRef(0)
  useEffect(() => {
    const media = ref.current
    if (!media || !visible) return
    const restore = () => {
      if (position.current < media.duration)
        media.currentTime = position.current
    }
    media.addEventListener("loadedmetadata", restore)
    media.src = src
    media.load()
    return () => {
      position.current = media.currentTime
      media.pause()
      media.removeEventListener("loadedmetadata", restore)
      media.removeAttribute("src")
      media.load()
    }
  }, [src, visible])
  useEffect(() => {
    if (ref.current) ref.current.playbackRate = rate
  }, [rate, visible])
  return ref
}
