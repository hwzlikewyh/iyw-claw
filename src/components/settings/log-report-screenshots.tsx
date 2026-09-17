"use client"

import { useRef } from "react"
import { ImagePlus, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  SCREENSHOT_TYPES,
  MAX_SCREENSHOTS,
  type ReportScreenshot,
} from "./use-report-screenshots"

interface Props {
  images: ReportScreenshot[]
  disabled: boolean
  onAdd: (files: File[]) => void
  onRemove: (id: string) => void
}

function ScreenshotPreview({
  image,
  disabled,
  onRemove,
}: {
  image: ReportScreenshot
  disabled: boolean
  onRemove: () => void
}) {
  const t = useTranslations("LogReport")
  return (
    <div className="relative aspect-[4/3] min-w-0 overflow-hidden rounded-md border bg-muted/30">
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img
        src={image.url}
        alt={image.file.name}
        className="h-full w-full object-contain"
      />
      <Button
        type="button"
        variant="secondary"
        size="icon"
        className="absolute right-1 top-1 h-6 w-6"
        disabled={disabled}
        onClick={onRemove}
        title={t("removeScreenshot")}
        aria-label={t("removeScreenshot")}
      >
        <X className="h-3.5 w-3.5" />
      </Button>
    </div>
  )
}

function AddScreenshot({
  disabled,
  onAdd,
}: {
  disabled: boolean
  onAdd: Props["onAdd"]
}) {
  const t = useTranslations("LogReport")
  const input = useRef<HTMLInputElement>(null)
  return (
    <>
      <Button
        type="button"
        variant="outline"
        className="aspect-[4/3] h-auto w-full border-dashed"
        disabled={disabled}
        onClick={() => input.current?.click()}
        title={t("addScreenshot")}
        aria-label={t("addScreenshot")}
      >
        <ImagePlus className="h-5 w-5" />
      </Button>
      <input
        ref={input}
        type="file"
        accept={SCREENSHOT_TYPES}
        multiple
        hidden
        onChange={(event) => {
          onAdd(Array.from(event.currentTarget.files ?? []))
          event.currentTarget.value = ""
        }}
      />
    </>
  )
}

export function LogReportScreenshots({
  images,
  disabled,
  onAdd,
  onRemove,
}: Props) {
  const t = useTranslations("LogReport")
  return (
    <fieldset className="space-y-2" disabled={disabled}>
      <legend className="text-sm font-medium">{t("screenshots")}</legend>
      <div
        className="grid grid-cols-3 gap-2 sm:grid-cols-4"
        onDragOver={(event) => event.preventDefault()}
        onDrop={(event) => {
          event.preventDefault()
          if (!disabled) onAdd(Array.from(event.dataTransfer.files))
        }}
      >
        {images.map((image) => (
          <ScreenshotPreview
            key={image.id}
            image={image}
            disabled={disabled}
            onRemove={() => onRemove(image.id)}
          />
        ))}
        <AddScreenshot
          disabled={disabled || images.length >= MAX_SCREENSHOTS}
          onAdd={onAdd}
        />
      </div>
    </fieldset>
  )
}
