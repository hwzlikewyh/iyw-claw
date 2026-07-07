"use client"

import { useEffect, useMemo, useState } from "react"
import { Check, Copy, Globe2, KeyRound, type LucideIcon } from "lucide-react"
import { useTranslations } from "next-intl"

import {
  getWebServerStatus,
  getWebServiceConfig,
  type WebServerInfo,
  type WebServiceConfig,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { useCopiedFlag } from "@/hooks/use-copied-flag"
import { copyTextToClipboard } from "@/lib/utils"

const DEFAULT_PORT = 3080

function CopyButton({ value, label }: { value: string | null; label: string }) {
  const [copied, markCopied] = useCopiedFlag()

  async function handleCopy() {
    if (!value) return
    const ok = await copyTextToClipboard(value)
    if (ok) markCopied()
  }

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      className="h-5 w-5 rounded-md text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground disabled:opacity-35"
      onClick={handleCopy}
      disabled={!value}
      title={label}
      aria-label={label}
    >
      {copied ? (
        <Check aria-hidden="true" className="h-3 w-3 text-green-600" />
      ) : (
        <Copy aria-hidden="true" className="h-3 w-3" />
      )}
    </Button>
  )
}

function InfoRow({
  icon: Icon,
  label,
  value,
  copyValue,
  copyLabel,
}: {
  icon: LucideIcon
  label: string
  value: string
  copyValue: string | null
  copyLabel: string
}) {
  return (
    <div className="flex min-w-0 items-center gap-1.5 px-1">
      <Icon
        aria-hidden="true"
        className="h-3 w-3 shrink-0 text-muted-foreground/80"
      />
      <span className="sr-only">{label}</span>
      <code className="min-w-0 flex-1 truncate font-mono text-[0.6875rem] text-muted-foreground">
        {value}
      </code>
      <CopyButton value={copyValue} label={copyLabel} />
    </div>
  )
}

export function SidebarWebAccess() {
  const t = useTranslations("WebServiceSettings")
  const [status, setStatus] = useState<WebServerInfo | null>(null)
  const [config, setConfig] = useState<WebServiceConfig | null>(null)

  useEffect(() => {
    let cancelled = false

    async function load() {
      const [statusResult, configResult] = await Promise.all([
        getWebServerStatus().catch(() => null),
        getWebServiceConfig().catch(() => null),
      ])
      if (cancelled) return
      setStatus(statusResult)
      setConfig(configResult)
    }

    const handleFocus = () => {
      void load()
    }

    void load()
    window.addEventListener("focus", handleFocus)
    return () => {
      cancelled = true
      window.removeEventListener("focus", handleFocus)
    }
  }, [])

  const port = status?.port ?? config?.port ?? DEFAULT_PORT
  const address = status?.addresses[0] ?? `http://localhost:${port}`
  const token = status?.token ?? config?.token ?? null
  const tokenDisplay = token ? "********" : "-"
  const isRunning = status !== null

  const statusLabel = isRunning ? t("running") : t("stopped")
  const copyAddressLabel = useMemo(
    () => `${t("copy")} ${t("addressLabel")}`,
    [t]
  )
  const copyTokenLabel = useMemo(() => `${t("copy")} ${t("tokenLabel")}`, [t])

  return (
    <div className="rounded-lg bg-sidebar-accent/35 px-2 py-2">
      <div className="flex min-w-0 items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5">
          <span
            aria-hidden="true"
            className={`h-1.5 w-1.5 shrink-0 rounded-full ${
              isRunning ? "bg-green-500" : "bg-muted-foreground/35"
            }`}
          />
          <span className="truncate text-[0.75rem] font-medium text-sidebar-foreground/85">
            {t("sectionTitle")}
          </span>
        </div>
        <span className="shrink-0 text-[0.625rem] text-muted-foreground">
          {statusLabel}
        </span>
      </div>
      <div className="mt-2 space-y-1">
        <InfoRow
          icon={Globe2}
          label={t("addressLabel")}
          value={address}
          copyValue={address}
          copyLabel={copyAddressLabel}
        />
        <InfoRow
          icon={KeyRound}
          label={t("tokenLabel")}
          value={tokenDisplay}
          copyValue={token}
          copyLabel={copyTokenLabel}
        />
      </div>
    </div>
  )
}
