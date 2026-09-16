"use client"

import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { marketItemFallbackInitial } from "@/lib/skill-market"
import { cn } from "@/lib/utils"

export function MarketItemIcon({
  name,
  src,
  className,
}: {
  name: string
  src?: string | null
  className?: string
}) {
  return (
    <Avatar
      className={cn(
        "size-10 shrink-0 overflow-hidden rounded-md after:hidden",
        className
      )}
      aria-hidden="true"
    >
      {src ? <AvatarImage className="rounded-none" src={src} alt="" /> : null}
      <AvatarFallback className="rounded-none bg-primary/10 text-base font-semibold text-primary">
        {marketItemFallbackInitial(name)}
      </AvatarFallback>
    </Avatar>
  )
}
