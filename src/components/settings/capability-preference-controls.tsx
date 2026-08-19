"use client"

import type { ReactNode } from "react"
import { AlertCircle, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { Badge } from "@/components/ui/badge"
import { Switch } from "@/components/ui/switch"

export function CapabilitySection({
  title,
  error,
  children,
}: {
  title: string
  error: string | null
  children: ReactNode
}) {
  return (
    <section className="space-y-2 border-b pb-4">
      <h4 className="text-sm font-medium">{title}</h4>
      <div className="divide-y rounded-md border">{children}</div>
      {error ? (
        <p className="flex items-start gap-1.5 break-words text-xs text-destructive">
          <AlertCircle className="mt-0.5 size-3.5 shrink-0" />
          {error}
        </p>
      ) : null}
    </section>
  )
}

export function PreferenceRow({
  id,
  label,
  checked,
  disabled,
  busy,
  mixed,
  denied,
  onCheckedChange,
}: {
  id: string
  label: string
  checked: boolean
  disabled: boolean
  busy: boolean
  mixed?: string | null
  denied: boolean
  onCheckedChange: (checked: boolean) => void
}) {
  const t = useTranslations("AcpAgentSettings.capabilities")
  return (
    <div className="flex min-h-11 items-center justify-between gap-3 px-3 py-2">
      <label htmlFor={id} className="min-w-0 text-xs font-medium">
        {label}
      </label>
      <div className="flex shrink-0 items-center gap-2">
        {mixed ? <Badge variant="outline">{mixed}</Badge> : null}
        {denied ? (
          <Badge variant="destructive">{t("managedDenied")}</Badge>
        ) : null}
        {busy ? <Loader2 className="size-3.5 animate-spin" /> : null}
        <Switch
          id={id}
          checked={checked}
          disabled={disabled}
          onCheckedChange={onCheckedChange}
          aria-label={label}
        />
      </div>
    </div>
  )
}
