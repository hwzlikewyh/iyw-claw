"use client"

import { useEffect, useRef, useState } from "react"

export function useProcessDisclosure(preferredOpen: boolean) {
  // 任务状态只决定默认值，手动展开或收起后保留用户选择。
  const [choice, setChoice] = useState<boolean | null>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const frame = useRef(0)
  useEffect(() => () => cancelAnimationFrame(frame.current), [])
  const onOpenChange = (open: boolean) => {
    cancelAnimationFrame(frame.current)
    setChoice(open)
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
  const open = choice ?? preferredOpen
  return { open, onOpenChange, triggerRef, hasChoice: choice !== null }
}
