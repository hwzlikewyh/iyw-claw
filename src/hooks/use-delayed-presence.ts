"use client"

import { useEffect, useRef, useState } from "react"

export function useDelayedPresence(visible: boolean, delayMs: number): boolean {
  const [retained, setRetained] = useState(visible)
  const generationRef = useRef(0)

  useEffect(() => {
    generationRef.current += 1
    const generation = generationRef.current
    if (visible) {
      const timer = window.setTimeout(() => {
        if (generationRef.current === generation) setRetained(true)
      }, 0)
      return () => window.clearTimeout(timer)
    }
    const timer = window.setTimeout(() => {
      if (generationRef.current === generation) setRetained(false)
    }, delayMs)
    return () => window.clearTimeout(timer)
  }, [delayMs, visible])

  return visible || retained
}
