"use client"

import { Globe2 } from "lucide-react"
import { Button } from "@/components/ui/button"

export function BrowserTitleButton({
  active,
  status,
  label,
  onClick,
  mobile = false,
}: {
  active: boolean
  status?: string
  label: string
  onClick: () => void
  mobile?: boolean
}) {
  const alert = status === "failed" || status === "missing"
  return (
    <Button
      variant="ghost"
      size="icon"
      className={`${mobile ? "h-8 w-8" : "h-6 w-6"} relative hover:text-foreground/80 ${active ? "bg-accent" : ""}`}
      onClick={onClick}
      title={label}
      aria-label={label}
      aria-pressed={active}
    >
      <Globe2 className={mobile ? "h-4 w-4" : "h-3.5 w-3.5"} />
      {alert ? (
        <span className="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-destructive" />
      ) : null}
    </Button>
  )
}
