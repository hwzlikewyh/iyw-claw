"use client"

import { useState, type ReactNode } from "react"

export function ScenarioCardIcon({
  source,
  fallback,
}: {
  source: string | null
  fallback: ReactNode
}) {
  const [failed, setFailed] = useState(false)
  const url = scenarioIconUrl(source)
  if (!url || failed) {
    return (
      <span className="flex size-9 shrink-0 items-center justify-center rounded-full bg-muted/45 text-foreground">
        {fallback}
      </span>
    )
  }
  return (
    // eslint-disable-next-line @next/next/no-img-element
    <img
      src={url}
      alt=""
      aria-hidden="true"
      width={36}
      height={36}
      className="size-9 shrink-0 object-contain"
      onError={() => {
        console.warn("[scenario-icon] image could not be loaded")
        setFailed(true)
      }}
    />
  )
}

function scenarioIconUrl(source: string | null) {
  if (!source?.startsWith("https://")) return null
  try {
    const url = new URL(source)
    return url.username || url.password ? null : url.href
  } catch {
    return null
  }
}
