"use client"

/* eslint-disable @next/next/no-img-element -- TOS/CDN URL is runtime-configured. */
import { cn } from "@/lib/utils"

export function ModelIcon({
  src,
  className,
}: {
  src?: string | null
  className?: string
}) {
  const value = src?.trim()
  if (!value) return null
  return (
    <img
      src={value}
      alt=""
      aria-hidden="true"
      loading="lazy"
      referrerPolicy="no-referrer"
      className={cn("size-4 shrink-0 rounded-sm object-cover", className)}
    />
  )
}
