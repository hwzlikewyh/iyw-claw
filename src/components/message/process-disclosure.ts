"use client"

import { useEffect, useRef, useState } from "react"

export function useProcessDisclosure(preferredOpen: boolean) {
  const [chosenOpen, setChosenOpen] = useState<boolean | null>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const frame = useRef(0)
  useEffect(() => () => cancelAnimationFrame(frame.current), [])
  const onOpenChange = (open: boolean) => {
    cancelAnimationFrame(frame.current)
    setChosenOpen(open)
    if (open) return
    const previousFocus = document.activeElement
    frame.current = requestAnimationFrame(() => {
      frame.current = requestAnimationFrame(() => {
        const trigger = triggerRef.current
        if (!trigger?.isConnected) return
        if (
          document.activeElement !== previousFocus &&
          document.activeElement !== document.body
        )
          return
        trigger.focus({ preventScroll: true })
        trigger.scrollIntoView({ block: "nearest", inline: "nearest" })
      })
    })
  }
  return { open: chosenOpen ?? preferredOpen, onOpenChange, triggerRef }
}
