"use client"

import { useTranslations } from "next-intl"
import type { RefObject } from "react"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { PageCaptureEditor } from "./page-capture-editor"
import type { NativePageController } from "./use-native-page"

const MOBILE_VIEWPORT_WIDTH = 390

export function NativePageFind({ view }: { view: NativePageController }) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  if (view.find === null) return null
  const search = (backwards = false) =>
    void view.operate("find", { query: view.find, backwards })
  return (
    <div className="flex shrink-0 items-center gap-1 border-b p-2">
      <Input
        autoFocus
        value={view.find}
        aria-label={t("find")}
        onChange={(event) => view.setFind(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") search(event.shiftKey)
          if (event.key === "Escape") {
            view.setFind(null)
            void view.operate("find", { query: "" })
          }
        }}
      />
      <Button size="sm" variant="ghost" onClick={() => search()}>
        {t("find")}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        onClick={() => {
          view.setFind(null)
          void view.operate("find", { query: "" })
        }}
      >
        {t("cancel")}
      </Button>
    </div>
  )
}

export function NativePageError({ view }: { view: NativePageController }) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  if (!view.error) return null
  return (
    <div
      role="alert"
      className="flex items-center gap-2 border-b p-2 text-xs text-destructive"
    >
      <span className="min-w-0 flex-1 break-words">{view.error}</span>
      <Button
        size="sm"
        variant="outline"
        onClick={() => {
          view.setError(null)
          view.setRevision(view.revision + 1)
        }}
      >
        {t("retry")}
      </Button>
    </div>
  )
}

export function NativePageSurface({
  view,
  surfaceRef,
}: {
  view: NativePageController
  surfaceRef: RefObject<HTMLDivElement | null>
}) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  return (
    <>
      {!view.native && (
        <p className="border-b p-2 text-xs text-muted-foreground">
          {t("webNotice")}
        </p>
      )}
      <div className="relative min-h-0 flex-1 overflow-auto bg-muted/30">
        <div
          ref={surfaceRef}
          className="mx-auto h-full min-h-40 bg-background"
          style={{
            width: view.mobile
              ? `min(100%,${MOBILE_VIEWPORT_WIDTH}px)`
              : "100%",
          }}
        >
          {!view.native && (
            <iframe
              title={view.state?.title ?? view.props.url}
              src={view.url}
              sandbox="allow-scripts allow-forms"
              referrerPolicy="no-referrer"
              className="h-full w-full border-0"
            />
          )}
        </div>
      </div>
    </>
  )
}

export function NativePageCapture({ view }: { view: NativePageController }) {
  if (!view.capture) return null
  const capture = view.capture
  return (
    <div className="absolute inset-0 z-10 bg-background">
      <PageCaptureEditor
        capture={capture.image}
        crop={capture.crop}
        onClose={() => view.setCapture(null)}
        onSend={(image, note) => view.sendCapture(image, note, capture.tabId)}
      />
    </div>
  )
}
