"use client"

import { useState } from "react"
import { Check, Copy, Eye, EyeOff, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { copyTextToClipboard } from "@/lib/utils"

export function ChannelSecretValueField({
  label,
  value,
  onRegenerate,
}: {
  label: string
  value: string
  onRegenerate?: () => void
}) {
  const t = useTranslations("ChatChannelSettings")
  const [visible, setVisible] = useState(false)
  const [copied, setCopied] = useState(false)

  const copy = async () => {
    if (!(await copyTextToClipboard(value))) return
    setCopied(true)
    window.setTimeout(() => setCopied(false), 1500)
  }

  return (
    <div className="min-w-0 space-y-1.5">
      <label className="text-xs font-medium">{label}</label>
      <div className="flex min-w-0 items-center gap-1 rounded-md border bg-muted/30 p-1 pl-3">
        <code className="min-w-0 flex-1 truncate text-xs">
          {visible ? value : "•".repeat(Math.min(value.length, 32))}
        </code>
        {onRegenerate && (
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            title={t("market.regenerate")}
            onClick={onRegenerate}
          >
            <RefreshCw className="h-3.5 w-3.5" />
          </Button>
        )}
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          title={visible ? t("market.hide") : t("market.show")}
          onClick={() => setVisible((current) => !current)}
        >
          {visible ? (
            <EyeOff className="h-3.5 w-3.5" />
          ) : (
            <Eye className="h-3.5 w-3.5" />
          )}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          title={t("market.copy")}
          onClick={() => void copy()}
        >
          {copied ? (
            <Check className="h-3.5 w-3.5 text-green-500" />
          ) : (
            <Copy className="h-3.5 w-3.5" />
          )}
        </Button>
      </div>
    </div>
  )
}
