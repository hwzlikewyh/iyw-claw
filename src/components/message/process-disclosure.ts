"use client"

import { useEffect, useRef, useState } from "react"

export function useProcessDisclosure(preferredOpen: boolean) {
  const [choice, setChoice] = useState<{
    preferred: boolean
    open: boolean | null
  }>({ preferred: preferredOpen, open: null })
  if (choice.preferred !== preferredOpen)
    setChoice({ preferred: preferredOpen, open: null })
  const triggerRef = useRef<HTMLButtonElement>(null)
  const frame = useRef(0)
  useEffect(() => () => cancelAnimationFrame(frame.current), [])
  const onOpenChange = (open: boolean) => {
    cancelAnimationFrame(frame.current)
    setChoice({ preferred: preferredOpen, open })
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
  const open =
    choice.preferred === preferredOpen
      ? (choice.open ?? preferredOpen)
      : preferredOpen
  return { open, onOpenChange, triggerRef }
}
