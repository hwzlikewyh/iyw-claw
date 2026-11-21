"use client"

import { formatSkillBytes } from "@/lib/skill-market"

export function DownloadProgress({
  label,
  receivedBytes,
  totalBytes,
}: {
  label: string
  receivedBytes?: number
  totalBytes?: number | null
}) {
  const total = totalBytes && totalBytes > 0 ? totalBytes : undefined
  const received =
    receivedBytes === undefined ? undefined : Math.max(0, receivedBytes)
  const percent =
    total && received !== undefined
      ? Math.min(100, Math.floor((received / total) * 100))
      : undefined

  return (
    <div className="grid gap-2">
      <div className="flex items-baseline justify-between gap-3 text-sm font-medium">
        <span>{label}</span>
        {percent !== undefined && (
          <span className="shrink-0 tabular-nums">{percent}%</span>
        )}
      </div>
      <progress
        aria-label={label}
        className="block h-2 w-full accent-primary"
        value={percent === undefined ? undefined : received}
        max={total}
      />
      {received !== undefined && (
        <p className="text-xs text-muted-foreground tabular-nums">
          {formatSkillBytes(received)}
          {total !== undefined && ` / ${formatSkillBytes(total)}`}
        </p>
      )}
    </div>
  )
}
