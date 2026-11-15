"use client"

import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { cn } from "@/lib/utils"
import { usePluginPresentation } from "./plugin-presentation"

export function PluginIcon({
  slug,
  label,
  className,
}: {
  slug: string
  label: string
  className?: string
}) {
  const metadata = usePluginPresentation().get(slug)
  const name = metadata?.label || label
  return (
    <Avatar
      aria-hidden="true"
      className={cn("size-7 shrink-0 rounded-md after:hidden", className)}
    >
      {metadata?.iconUrl && (
        <AvatarImage
          src={metadata.iconUrl}
          alt=""
          className="rounded-none object-contain"
        />
      )}
      <AvatarFallback className="rounded-md bg-muted text-[11px] font-medium text-muted-foreground">
        {pluginInitials(name)}
      </AvatarFallback>
    </Avatar>
  )
}

function pluginInitials(label: string) {
  const words = label
    .trim()
    .replace(/^(iyw-)?zcode-|^iyw-/i, "")
    .split(/[\s-]+/)
  const initials =
    words.length > 1 && words.every((word) => /^[a-z]/i.test(word))
      ? words.map((word) => Array.from(word)[0]).join("")
      : words[0]
  return Array.from(initials || "?")
    .slice(0, 2)
    .join("")
    .toLocaleUpperCase()
}
