"use client"

import { Crosshair, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { cn } from "@/lib/utils"
import {
  elementReferenceType,
  readElementReference,
} from "../element-reference"
import type { ReferenceAttrs } from "../types"

function useElementLabel(data: ReferenceAttrs) {
  const t = useTranslations("Browser")
  const details = readElementReference(data.uri)
  const type = t(
    `elementTypes.${elementReferenceType(details?.tag, details?.role)}`
  )
  const label = data.label || details?.selector || data.id
  const title = [
    `${type} · ${label}`,
    details ? t("elementTag", { tag: details.tag }) : null,
    details?.role ? t("elementRole", { role: details.role }) : null,
    details?.selector
      ? t("elementSelector", { selector: details.selector })
      : null,
    details?.text ? t("elementText", { text: details.text }) : null,
    details?.url,
  ]
    .filter(Boolean)
    .join("\n")
  return {
    type,
    label,
    title,
    removeLabel: t("removeElement", { name: label }),
  }
}

export function ElementReferenceBadge({
  data,
  className,
  onRemove,
}: {
  data: ReferenceAttrs
  className?: string
  onRemove?: () => void
}) {
  const { type, label, title, removeLabel } = useElementLabel(data)
  return (
    <span
      className={cn(
        "inline-flex max-w-full items-center gap-1 align-middle",
        className
      )}
    >
      <ElementLabel type={type} label={label} title={title} />
      {onRemove && <RemoveElement label={removeLabel} onRemove={onRemove} />}
    </span>
  )
}

function ElementLabel({
  type,
  label,
  title,
}: {
  type: string
  label: string
  title: string
}) {
  return (
    <span
      data-reference-badge=""
      data-ref-type="element"
      role="img"
      aria-label={`${type}: ${label}`}
      title={title}
      className="inline-flex min-w-0 max-w-full items-center gap-1.5 align-middle text-[0.82em] leading-normal text-muted-foreground"
    >
      <Crosshair className="size-3 shrink-0" aria-hidden />
      <span className="shrink-0 text-[0.9em]">{type}</span>
      <span className="h-2.5 w-px shrink-0 bg-border" aria-hidden />
      <span className="max-w-56 truncate text-foreground/75">{label}</span>
    </span>
  )
}

function RemoveElement({
  label,
  onRemove,
}: {
  label: string
  onRemove: () => void
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className="shrink-0 rounded-sm text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
      onMouseDown={(event) => event.preventDefault()}
      onClick={(event) => {
        event.preventDefault()
        event.stopPropagation()
        onRemove()
      }}
    >
      <X className="size-3" aria-hidden />
    </button>
  )
}
