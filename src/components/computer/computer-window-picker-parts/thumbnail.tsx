"use client"

import { useEffect } from "react"
import Image from "next/image"
import { AppWindow, Loader2 } from "lucide-react"
import { computerWindowThumbnail } from "@/lib/computer/computer-api"
import {
  useComputerHostState,
  useComputerTransport,
} from "@/lib/computer/use-computer-host"

export function Thumbnail({ targetId }: { targetId: string }) {
  const [src, setSrc] = useComputerHostState<string | null | undefined>(
    undefined
  )
  const transport = useComputerTransport()
  useEffect(() => {
    let cancelled = false
    computerWindowThumbnail(targetId)
      .then((url) => {
        if (!cancelled) setSrc(url)
      })
      .catch(() => {
        if (!cancelled) setSrc(null)
      })
    return () => {
      cancelled = true
    }
  }, [targetId, transport, setSrc])
  return (
    <div className="relative aspect-video w-full bg-muted/60">
      {src ? (
        <Image
          src={src}
          alt=""
          width={320}
          height={180}
          unoptimized
          className="absolute inset-0 m-auto max-h-[calc(100%-1rem)] max-w-[calc(100%-1rem)] rounded-[3px] shadow-sm ring-1 ring-black/5 dark:ring-white/10"
        />
      ) : (
        <div className="absolute inset-0 flex items-center justify-center">
          {src === undefined ? (
            <Loader2 className="size-4 animate-spin text-muted-foreground" />
          ) : (
            <AppWindow className="size-6 text-muted-foreground/50" />
          )}
        </div>
      )}
    </div>
  )
}
