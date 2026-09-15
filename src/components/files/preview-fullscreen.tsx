"use client"

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react"
import { createPortal } from "react-dom"
import { cn } from "@/lib/utils"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"

function usePreviewContainer(open: boolean) {
  const [container, setContainer] = useState<HTMLDivElement | null>(null)
  const inlineRef = useRef<HTMLDivElement>(null)
  const modalRef = useRef<HTMLDivElement>(null)
  const place = useCallback(() => {
    const target = open ? modalRef.current : inlineRef.current
    if (target && container && container.parentElement !== target)
      target.appendChild(container)
  }, [container, open])
  const attachInline = useCallback(
    (node: HTMLDivElement | null) => {
      inlineRef.current = node
      if (!node || container) return
      const element = document.createElement("div")
      element.className = "h-full min-h-0"
      node.appendChild(element)
      setContainer(element)
    },
    [container]
  )
  useEffect(() => () => container?.remove(), [container])
  useEffect(place, [place])
  return { container, inlineRef, modalRef, attachInline, place }
}

export function PreviewFullscreen({
  open,
  onClose,
  title,
  children,
  className,
}: {
  open: boolean
  onClose: () => void
  title: string
  children: ReactNode
  className?: string
}) {
  const { container, inlineRef, modalRef, attachInline, place } =
    usePreviewContainer(open)
  return (
    <>
      <div ref={attachInline} className={cn("h-full min-h-0", className)} />
      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (!next) onClose()
        }}
      >
        <DialogContent
          className="fixed inset-0 h-dvh max-h-none w-dvw max-w-none overflow-hidden rounded-none p-0 sm:max-w-none"
          showCloseButton={false}
          onCloseAutoFocus={() => {
            if (container) inlineRef.current?.appendChild(container)
          }}
        >
          <DialogTitle className="sr-only">{title}</DialogTitle>
          <DialogDescription className="sr-only">{title}</DialogDescription>
          <div
            ref={(node) => {
              modalRef.current = node
              place()
            }}
            className="h-full min-h-0"
          />
        </DialogContent>
      </Dialog>
      {container && createPortal(children, container)}
    </>
  )
}
