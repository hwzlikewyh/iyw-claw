"use client"

import { useEffect, useState } from "react"
import { BarChart3, Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { toErrorMessage } from "@/lib/app-error"
import {
  getMemoryEffectiveness,
  type MemoryEffectivenessStatus,
} from "@/lib/user-memory-authority"

export function UserMemoryEffectivenessPanel() {
  const t = useTranslations("UserMemorySettings.effectiveness")
  const [data, setData] = useState<MemoryEffectivenessStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const load = async () => {
    setBusy(true)
    setError(null)
    try {
      setData(await getMemoryEffectiveness())
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      setBusy(false)
    }
  }
  useEffect(() => {
    void load()
  }, [])
  return (
    <section className="space-y-3 border-b py-4">
      <div className="flex items-center justify-between gap-3">
        <h2 className="flex items-center gap-2 text-sm font-medium">
          <BarChart3 className="size-4" />
          {t("title")}
        </h2>
        <Button
          size="icon"
          variant="ghost"
          title={t("refresh")}
          aria-label={t("refresh")}
          disabled={busy}
          onClick={() => void load()}
        >
          {busy ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <RefreshCw className="size-4" />
          )}
        </Button>
      </div>
      {error && <p className="text-xs text-destructive">{error}</p>}
      {data && <EffectivenessStats data={data} />}
      <p className="text-xs leading-5 text-muted-foreground">{t("boundary")}</p>
    </section>
  )
}

function EffectivenessStats({ data }: { data: MemoryEffectivenessStatus }) {
  const t = useTranslations("UserMemorySettings.effectiveness")
  const values = [
    ["deliveries", data.deliveries],
    ["reviewed", data.reviewed],
    ["used", data.used],
    ["irrelevant", data.irrelevant],
    ["outdated", data.outdated],
    ["unreviewed", data.unreviewed],
  ] as const
  return (
    <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
      {values.map(([label, value]) => (
        <span key={label}>
          {t(label)}: {value}
        </span>
      ))}
    </div>
  )
}
