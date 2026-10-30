"use client"

import { useEffect, useRef, useState, type RefObject } from "react"
import { createPortal } from "react-dom"
import { toPng } from "html-to-image"
import { toast } from "sonner"
import { downloadImage } from "@/lib/image-download"
import { toErrorMessage } from "@/lib/app-error"
import type { useShareComposer } from "./use-share-composer"
import { ShareThread } from "./share-thread"
import { prepareShareFiles } from "@/lib/conversation-share/prepare"

type Composer = ReturnType<typeof useShareComposer>
const IMAGE_WIDTH = 840
const MAX_IMAGE_HEIGHT = 16000
type ImageAssets = Awaited<ReturnType<typeof prepareShareFiles>> & {
  urls: Map<string, string>
}

export function ShareImageExport({
  state,
  done,
}: {
  state: Composer
  done: () => void
}) {
  const root = useRef<HTMLDivElement>(null)
  const [capture] = useState(() => state)
  const [assets, setAssets] = useState<ImageAssets | null>(null)
  const { candidates, fileIDs, title } = capture
  useEffect(() => {
    let current = true
    const urls = new Map<string, string>()
    void (async () => {
      const prepared = await prepareShareFiles(
        candidates.filter((file) => fileIDs.has(file.reference))
      )
      if (!current) return
      prepared.blobs.forEach((blob, key) =>
        urls.set(key, URL.createObjectURL(blob))
      )
      setAssets({ ...prepared, urls })
    })().catch((error) => {
      if (current) {
        toast.error(toErrorMessage(error))
        done()
      }
    })
    return () => {
      current = false
      urls.forEach((url) => URL.revokeObjectURL(url))
    }
  }, [candidates, fileIDs, title, done])
  useEffect(() => {
    if (!assets) return
    let current = true
    const frame = requestAnimationFrame(() => {
      if (!root.current) return done()
      void saveShareImage(root.current, title)
        .catch((error) => {
          if (current) toast.error(toErrorMessage(error))
        })
        .finally(() => {
          if (current) done()
        })
    })
    return () => {
      current = false
      cancelAnimationFrame(frame)
    }
  }, [assets, title, done])
  if (!capture.snapshot) return null
  return <ImageCanvas state={capture} assets={assets} root={root} />
}

function ImageCanvas({
  state,
  assets,
  root,
}: {
  state: Composer
  assets: ImageAssets | null
  root: RefObject<HTMLDivElement | null>
}) {
  if (!state.snapshot) return null
  return createPortal(
    <div
      aria-hidden
      className="fixed top-0 left-[-10000px] pointer-events-none"
      style={{ width: IMAGE_WIDTH }}
    >
      <div
        ref={root}
        className="bg-background px-4 py-6 text-foreground"
        style={{ width: IMAGE_WIDTH }}
      >
        <p className="px-4 text-xs text-muted-foreground">原助理 · 会话分享</p>
        <h1 className="px-4 py-2 text-xl font-semibold">{state.title}</h1>
        <ShareThread
          snapshot={state.snapshot}
          files={assets?.specs ?? []}
          fileURL={(reference) => assets?.urls.get(reference)}
        />
      </div>
    </div>,
    document.body
  )
}

async function saveShareImage(element: HTMLElement, title: string) {
  await document.fonts.ready
  await Promise.all(
    Array.from(element.querySelectorAll("img"), (image) => image.decode())
  )
  if (element.scrollHeight > MAX_IMAGE_HEIGHT)
    throw new Error("所选内容过长，请减少勾选的消息后再生成长图。")
  const source = await toPng(element, {
    width: IMAGE_WIDTH,
    pixelRatio: 1,
    backgroundColor: getComputedStyle(element).backgroundColor,
  })
  const saved = await downloadImage({
    data: source.slice(source.indexOf(",") + 1),
    mime_type: "image/png",
    suggestedName: `${title.replace(/[<>:"/\\|?*]/g, "_").slice(0, 80) || "会话分享"}.png`,
  })
  if (saved) toast.success("长图已保存")
}
