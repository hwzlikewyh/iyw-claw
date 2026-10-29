"use client"

import type { ReactNode } from "react"
import { Checkbox } from "@/components/ui/checkbox"
import { useConversationShare } from "./conversation-share-context"

export function ShareMessageSelection({
  messageIDs,
  role,
  className,
  children,
}: {
  messageIDs: string[]
  role: "user" | "assistant" | "system"
  className?: string
  children: ReactNode
}) {
  const state = useConversationShare()?.state
  const eligible = messageIDs.filter((id) => state?.availableIDs.has(id))
  const selecting = !!state && !state.preview && !state.result
  const checked = eligible.filter((id) => state?.ids.has(id)).length
  return (
    <div className={selecting ? "flex min-w-0 items-start gap-3" : "min-w-0"}>
      {selecting && role !== "system" && (
        <Checkbox
          className="mt-3 shrink-0"
          aria-label={`选择分享${role === "user" ? "提问" : "回答"}`}
          checked={
            checked && checked === eligible.length
              ? true
              : checked
                ? "indeterminate"
                : false
          }
          disabled={!eligible.length || !!state.status}
          onCheckedChange={(value) =>
            state.toggleMessages(eligible, value === true)
          }
        />
      )}
      <div className={`min-w-0 flex-1 ${className ?? ""}`}>{children}</div>
    </div>
  )
}
