"use client"

import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Box, Plug, Sparkles } from "lucide-react"
import { cn } from "@/lib/utils"

export function MarketItemIcon({
  name,
  src,
  className,
  kind = "skill",
}: {
  name: string
  src?: string | null
  className?: string
  kind?: "skill" | "connector" | "plugin"
}) {
  const Icon = kind === "plugin" ? Box : kind === "connector" ? Plug : Sparkles
  return (
    <Avatar
      className={cn(
        "size-10 shrink-0 overflow-hidden rounded-md after:hidden",
        className
      )}
      aria-hidden="true"
    >
      {src ? <AvatarImage className="rounded-none" src={src} alt="" /> : null}
      <AvatarFallback
        className="rounded-none bg-transparent text-muted-foreground"
        title={name}
      >
        <Icon className="size-full max-h-7 max-w-7" strokeWidth={1.5} />
      </AvatarFallback>
    </Avatar>
  )
}
