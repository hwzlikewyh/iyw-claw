"use client"

import type { ReactNode } from "react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

export function SkillMarketLocalToolDialog({
  open,
  title,
  children,
  onOpenChange,
}: {
  open: boolean
  title: string
  children: ReactNode
  onOpenChange: (open: boolean) => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="h-[min(48rem,calc(100dvh-2rem))] max-w-6xl overflow-hidden rounded-lg p-0">
        <DialogHeader className="sr-only">
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{title}</DialogDescription>
        </DialogHeader>
        {children}
      </DialogContent>
    </Dialog>
  )
}
