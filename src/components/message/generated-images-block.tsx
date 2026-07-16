"use client"

import { memo, useCallback, useState } from "react"
import Image from "next/image"
import {
  AlertCircle,
  Download,
  ExternalLink,
  ImageIcon,
  ImagePlus,
} from "lucide-react"
import { useTranslations } from "next-intl"
import type { UserImageDisplay } from "@/lib/adapters/ai-elements-adapter"
import type { ToolCallStatus } from "@/lib/types"
import { ImagePreviewDialog } from "@/components/ui/image-preview-dialog"
import { downloadImage } from "@/lib/image-download"
import { toErrorMessage } from "@/lib/app-error"
import { cn } from "@/lib/utils"
import { isLocalDesktop, openPath, openUrl } from "@/lib/platform"

export type ImagePresentation = "generated" | "displayed"

export interface ImageSourceLink {
  kind: "file" | "url"
  target: string
  label: string
}

interface GeneratedImagesBlockProps {
  /**
   * codex's revised prompt — what the model rewrote the user's request
   * into before passing to the image API. `null` when codex didn't echo
   * one back (e.g. failed generations) or it hasn't streamed in yet.
   */
  revisedPrompt: string | null
  caption?: string | null
  presentation?: ImagePresentation
  sourceLink?: ImageSourceLink | null
  /**
   * `null` while the agent has emitted the ToolCall but the image hasn't
   * arrived. The component renders an image-shaped skeleton placeholder
   * for this case so the user sees acknowledgement of work in progress
   * before the (often multi-second) image bytes land.
   *
   * NB: with codex-rs versions that don't emit `image_generation_begin`
   * events (≤ 0.122.0 at time of writing), there is no in-flight window
   * — `image` arrives populated on the very first tool_call event. The
   * placeholder branch is reached only when codex-rs upgrades to a
   * version that emits begin events.
   */
  image: UserImageDisplay | null
  /**
   * Live tool-call status. When `image` is null and `status === "failed"`,
   * the renderer shows a failure slot instead of a perpetual skeleton.
   * `null` (Rust-emitted JSONL replay blocks) is treated as success — by
   * definition such blocks always carry a present `image`.
   */
  status?: ToolCallStatus | null
  className?: string
}

/**
 * Renders one generated or agent-displayed image as a labeled, in-position
 * card with an optional prompt/caption and external source link.
 *
 * Layout uses a container query (`@container/genimg`):
 *   - wide (≥ 28rem available): prompt on the left, image on the right
 *     (so the prompt fills the empty space next to a square thumbnail)
 *   - narrow: image stacks below the prompt, mirroring the original
 *     vertical layout
 *
 * Distinct from regular tool-call cards so it never folds into a
 * `tool-group` collapsible.
 *
 * Download is platform-aware:
 *   - desktop: native "Save As" via Tauri command
 *   - web: blob `<a download>`
 */
export const GeneratedImagesBlock = memo(function GeneratedImagesBlock({
  revisedPrompt,
  caption,
  presentation = "generated",
  sourceLink,
  image,
  status,
  className,
}: GeneratedImagesBlockProps) {
  const t = useTranslations("Folder.chat.messageList")
  const [previewOpen, setPreviewOpen] = useState(false)
  // Treat `failed` (and the unusual `completed`-without-image case) as
  // failure so the user gets a clear error indicator instead of a
  // perpetual skeleton when codex reports the call ended without an image.
  const isFailed =
    image === null && (status === "failed" || status === "completed")

  const handleDownload = useCallback(
    async (img: UserImageDisplay) => {
      try {
        await downloadImage({
          data: img.data,
          mime_type: img.mime_type,
          suggestedName: img.name,
        })
      } catch (err) {
        const message = toErrorMessage(err)
        window.alert(t("downloadFailed", { message }))
      }
    },
    [t]
  )

  const trimmedPrompt =
    typeof revisedPrompt === "string" ? revisedPrompt.trim() : ""
  const trimmedCaption = typeof caption === "string" ? caption.trim() : ""
  const description =
    presentation === "displayed" ? trimmedCaption : trimmedPrompt
  const canOpenSource =
    sourceLink?.kind === "url" ||
    (sourceLink?.kind === "file" && isLocalDesktop())

  const handleOpenSource = useCallback(async () => {
    if (!sourceLink || !canOpenSource) return
    try {
      if (sourceLink.kind === "url") {
        await openUrl(sourceLink.target)
      } else {
        await openPath(sourceLink.target)
      }
    } catch (err) {
      window.alert(t("imageSourceOpenFailed", { message: toErrorMessage(err) }))
    }
  }, [canOpenSource, sourceLink, t])

  return (
    <div
      className={cn(
        "@container/genimg rounded-md border border-border/70 bg-muted/20 p-3",
        className
      )}
    >
      <div className="flex items-center gap-1.5 text-sm font-medium text-foreground">
        {presentation === "displayed" ? (
          <ImageIcon className="h-3.5 w-3.5 text-primary" />
        ) : (
          <ImagePlus className="h-3.5 w-3.5 text-primary" />
        )}
        <span>
          {t(
            presentation === "displayed" ? "imageDisplayed" : "imageGeneration"
          )}
        </span>
      </div>

      <div className="mt-2.5 flex flex-col gap-3 @[28rem]/genimg:flex-row @[28rem]/genimg:items-start">
        {description.length > 0 ? (
          <div className="min-w-0 flex-1 whitespace-pre-wrap break-words text-xs text-muted-foreground">
            {description}
          </div>
        ) : null}

        {image ? (
          <div className="group relative inline-block shrink-0 overflow-hidden rounded-md border border-border/70 bg-muted/30">
            <button
              type="button"
              onClick={() => setPreviewOpen(true)}
              className="block cursor-pointer transition-opacity hover:opacity-80"
            >
              <Image
                src={`data:${image.mime_type};base64,${image.data}`}
                alt={image.name}
                width={256}
                height={256}
                unoptimized
                className="h-auto max-h-64 w-auto max-w-full object-contain"
              />
            </button>
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation()
                void handleDownload(image)
              }}
              className="absolute right-1 top-1 rounded-full bg-background/80 p-1 text-foreground/80 opacity-0 shadow-sm transition-opacity hover:bg-background hover:text-foreground group-hover:opacity-100 focus-visible:opacity-100"
              aria-label={t("downloadImage")}
              title={t("downloadImage")}
            >
              <Download className="h-3.5 w-3.5" />
            </button>
          </div>
        ) : isFailed ? (
          <div
            className="flex h-64 w-64 max-w-full shrink-0 items-center justify-center rounded-md border border-dashed border-destructive/40 bg-destructive/5 text-xs text-destructive"
            role="status"
            aria-live="polite"
          >
            <div className="flex flex-col items-center gap-1.5">
              <AlertCircle className="h-6 w-6 opacity-80" />
              <span>{t("imageGenerationFailed")}</span>
            </div>
          </div>
        ) : (
          <div
            className="flex h-64 w-64 max-w-full shrink-0 animate-pulse items-center justify-center rounded-md border border-dashed border-border/70 bg-muted/40 text-xs text-muted-foreground"
            role="status"
            aria-live="polite"
          >
            <div className="flex flex-col items-center gap-1.5">
              <ImagePlus className="h-6 w-6 opacity-60" />
              <span>{t("imageGenerationPending")}</span>
            </div>
          </div>
        )}
      </div>

      {sourceLink ? (
        <div className="mt-2.5 flex min-w-0 items-start gap-1.5 border-t border-border/60 pt-2 text-xs text-muted-foreground">
          <span className="shrink-0">{t("imageSource")}:</span>
          {canOpenSource ? (
            <button
              type="button"
              onClick={() => void handleOpenSource()}
              className="flex min-w-0 items-start gap-1 text-left text-foreground/80 underline-offset-2 hover:text-foreground hover:underline"
              title={sourceLink.target}
            >
              <span className="break-all">{sourceLink.label}</span>
              <ExternalLink className="mt-0.5 h-3 w-3 shrink-0" />
            </button>
          ) : (
            <span className="min-w-0">
              <span className="break-all text-foreground/80">
                {sourceLink.label}
              </span>{" "}
              <span>({t("imageSourceUnavailable")})</span>
            </span>
          )}
        </div>
      ) : null}

      <ImagePreviewDialog
        src={image ? `data:${image.mime_type};base64,${image.data}` : ""}
        alt={image?.name ?? ""}
        open={previewOpen && image !== null}
        onOpenChange={(open) => setPreviewOpen(open)}
        onDownload={image ? () => void handleDownload(image) : undefined}
        downloadLabel={t("downloadImage")}
      />
    </div>
  )
})
