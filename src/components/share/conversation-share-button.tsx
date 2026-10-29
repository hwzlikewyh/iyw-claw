"use client"

import { Share2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { useConversationShare } from "./conversation-share-context"

export function ConversationShareButton() {
  const share = useConversationShare()
  if (!share) return null
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label="分享会话"
          aria-pressed={!!share.state}
          disabled={!!share.state?.status}
          onClick={share.state ? share.close : share.start}
        >
          <Share2 className="size-4" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>分享会话</TooltipContent>
    </Tooltip>
  )
}
