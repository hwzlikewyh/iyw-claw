"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { openUrl } from "@/lib/platform"
import type { ScenarioCaseAsset } from "@/lib/scenario-cases"

export function ScenarioCaseSource({ asset }: { asset: ScenarioCaseAsset }) {
  const [failed, setFailed] = useState(false)
  return (
    <div className="space-y-2 p-4 text-sm">
      {asset.kind === "file" && (
        <>
          <p className="break-all font-medium">
            {asset.label || asset.fileName}
          </p>
          <p className="text-muted-foreground">
            此文件可下载后查看；在线预览请上传 PDF 或图片。
          </p>
        </>
      )}
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() => {
          setFailed(false)
          void openUrl(asset.url).catch(() => {
            console.warn("[scenario-case] open original failed", {
              kind: asset.kind,
            })
            setFailed(true)
          })
        }}
      >
        打开或下载原文件
      </Button>
      {failed && <p role="alert">原文件打开失败，请重试。</p>}
    </div>
  )
}
