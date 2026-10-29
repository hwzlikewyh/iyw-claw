"use client"

import { useEffect, useState } from "react"
import { Button } from "@/components/ui/button"
import { shareAction } from "@/lib/conversation-share/client"
import { toErrorMessage } from "@/lib/app-error"
import type { ShareRecord } from "@/lib/conversation-share/types"

export function ShareHistory({ conversationId }: { conversationId: number }) {
  const [items, setItems] = useState<ShareRecord[] | null>(null)
  const [error, setError] = useState("")
  const [pending, setPending] = useState("")
  const [revision, setRevision] = useState(0)
  useEffect(() => {
    let current = true
    shareAction<{ items: ShareRecord[] }>("list", {
      sourceConversationId: String(conversationId),
    })
      .then((value) => {
        if (current) setItems(value.items)
      })
      .catch((err) => {
        if (current) setError(toErrorMessage(err))
      })
    return () => {
      current = false
    }
  }, [conversationId, revision])
  const revoke = async (id: string) => {
    setPending(id)
    setError("")
    try {
      await shareAction("revoke", { id })
      setRevision((value) => value + 1)
    } catch (err) {
      setError(toErrorMessage(err))
    } finally {
      setPending("")
    }
  }
  return (
    <div className="space-y-3 py-2">
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
          <Button
            variant="link"
            onClick={() => setRevision((value) => value + 1)}
          >
            重试
          </Button>
        </p>
      )}
      {!items && !error && <p role="status">正在加载分享记录…</p>}
      {items?.length === 0 && (
        <p className="text-sm text-muted-foreground">此会话还没有分享记录。</p>
      )}
      {items?.map((item) => {
        const expired =
          !!item.expiresAt && Date.parse(item.expiresAt) <= Date.now()
        const state =
          item.state === "revoked"
            ? "已撤销"
            : expired
              ? "已过期"
              : item.state === "draft"
                ? "未发布"
                : "分享中"
        return (
          <div
            key={item.id}
            className="flex items-center justify-between gap-3 border-b py-2 text-sm"
          >
            <div className="min-w-0">
              <p className="truncate">{item.title}</p>
              <p className="text-xs text-muted-foreground">
                {state} ·{" "}
                {item.expiresAt
                  ? new Date(item.expiresAt).toLocaleString()
                  : "长期"}
              </p>
            </div>
            <Button
              variant="outline"
              size="sm"
              disabled={!!pending || item.state === "revoked" || expired}
              onClick={() => void revoke(item.id)}
            >
              {pending === item.id ? "正在撤销…" : "撤销分享"}
            </Button>
          </div>
        )
      })}
    </div>
  )
}
