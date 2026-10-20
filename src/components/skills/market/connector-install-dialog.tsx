"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  mcpGetMarketplaceServerDetail,
  mcpInstallFromMarketplace,
} from "@/lib/api"
import type {
  McpMarketplaceItem,
  McpMarketplaceServerDetail,
} from "@/lib/types"

export function ConnectorInstallDialog({
  item,
  onClose,
  onInstalled,
}: {
  item: McpMarketplaceItem
  onClose: () => void
  onInstalled: () => void
}) {
  const t = useTranslations("ConnectorMarket")
  const [detail, setDetail] = useState<McpMarketplaceServerDetail | null>(null)
  const [optionId, setOptionId] = useState("")
  const [values, setValues] = useState<Record<string, string>>({})
  const [error, setError] = useState("")
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    let active = true
    mcpGetMarketplaceServerDetail({
      providerId: item.provider_id,
      serverId: item.server_id,
    })
      .then((value) => {
        if (active) {
          setDetail(value)
          setOptionId(value.default_option_id ?? "")
        }
      })
      .catch((cause) => {
        if (active) setError(String(cause))
      })
    return () => {
      active = false
    }
  }, [item.provider_id, item.server_id])
  const option = detail?.install_options.find((value) => value.id === optionId)
  const installation = option?.spec.iyw_install as
    | { runtime?: string; artifact?: boolean; auth_mode?: string }
    | undefined
  const install = async () => {
    if (!option) return
    setBusy(true)
    setError("")
    try {
      const parameters: Record<string, unknown> = {}
      for (const field of option.parameters) {
        const raw = values[field.key]
        if (raw == null || raw === "") {
          if (field.required && field.default_value == null)
            throw new Error(`${field.label}: ${t("required")}`)
          continue
        }
        parameters[field.key] = parseValue(raw, field.kind)
      }
      await mcpInstallFromMarketplace({
        providerId: item.provider_id,
        serverId: item.server_id,
        optionId: option.id,
        protocol: option.protocol,
        parameterValues: parameters,
      })
      onInstalled()
    } catch (cause) {
      setError(String(cause))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent className="max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{item.name}</DialogTitle>
          <DialogDescription className="whitespace-pre-line">
            {detail?.description ?? item.description}
          </DialogDescription>
        </DialogHeader>
        <p className="text-xs text-muted-foreground">{t("installHint")}</p>
        {detail && !detail.install_options.length && (
          <p role="alert" className="text-sm text-destructive">
            {t("incompatible")}
          </p>
        )}
        {option?.description && (
          <p className="text-xs text-muted-foreground">{option.description}</p>
        )}
        {option && (
          <div className="space-y-1 rounded-md bg-muted p-3 text-xs">
            <p>
              {t("runtime")}:{" "}
              {option.protocol === "stdio"
                ? t("runtime_local")
                : t("runtime_remote")}
            </p>
            <p>
              {t("authMode")}: {installation?.auth_mode || "none"}
            </p>
            {installation?.artifact && <p>{t("artifactDownload")}</p>}
          </div>
        )}
        {detail && (
          <label className="grid gap-2 text-sm">
            {t("connectionOption")}
            <select
              value={optionId}
              disabled={busy}
              className="h-9 rounded-md border bg-background px-2"
              onChange={(event) => {
                setOptionId(event.target.value)
                setValues({})
              }}
            >
              {detail.install_options.map((value) => (
                <option key={value.id} value={value.id}>
                  {value.label}
                </option>
              ))}
            </select>
          </label>
        )}
        {option?.parameters.map((field) => (
          <div key={field.key} className="grid gap-1.5">
            <Label htmlFor={`connector-param-${field.key}`}>
              {field.label}
              {field.required ? " *" : ""}
            </Label>
            <Input
              id={`connector-param-${field.key}`}
              type={field.secret ? "password" : "text"}
              autoComplete="off"
              placeholder={
                field.placeholder ??
                (field.default_value == null ? "" : String(field.default_value))
              }
              value={values[field.key] ?? ""}
              disabled={busy}
              onChange={(event) =>
                setValues((current) => ({
                  ...current,
                  [field.key]: event.target.value,
                }))
              }
            />
            {field.description && (
              <p className="text-xs text-muted-foreground">
                {field.description}
              </p>
            )}
          </div>
        ))}
        {error && (
          <p role="alert" className="text-xs text-destructive">
            {error}
          </p>
        )}
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button disabled={busy || !option} onClick={() => void install()}>
            {busy ? t("installing") : t("install")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function parseValue(raw: string, kind: string): unknown {
  if (kind === "boolean" || kind === "json") return JSON.parse(raw)
  if (kind === "number" || kind === "integer") {
    const value = Number(raw)
    if (
      !Number.isFinite(value) ||
      (kind === "integer" && !Number.isInteger(value))
    )
      throw new Error("Invalid number")
    return value
  }
  return raw
}
