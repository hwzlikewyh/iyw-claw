import type { ReactNode } from "react"
import { MessageCircleQuestionMark } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { ConversationShareButton } from "@/components/share/conversation-share-button"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"

const MAX_TITLE_LENGTH = 12

export function ConversationTaskToolbar({
  title,
  task,
  onSideQuestion,
}: {
  title?: string
  task?: ReactNode
  onSideQuestion?: (question: string) => boolean
}) {
  const characters = Array.from(title ?? "")
  const displayTitle =
    characters.length > MAX_TITLE_LENGTH
      ? `${characters.slice(0, MAX_TITLE_LENGTH).join("")}…`
      : title
  return (
    <div className="conversation-task-toolbar">
      {title && (
        <h1
          className="conversation-toolbar-title"
          title={title}
          aria-label={title}
        >
          {displayTitle}
        </h1>
      )}
      <div className="conversation-task-toolbar-slot">{task}</div>
      <div className="conversation-toolbar-actions flex shrink-0 items-center gap-1">
        {onSideQuestion && <SideQuestionButton onAsk={onSideQuestion} />}
        <ConversationShareButton />
      </div>
    </div>
  )
}

function SideQuestionButton({
  onAsk,
}: {
  onAsk: (question: string) => boolean
}) {
  const t = useTranslations("SideQuestion")
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          className="conversation-side-question"
          aria-label={t("title")}
          onClick={() => onAsk("")}
        >
          <MessageCircleQuestionMark className="size-4" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{t("title")}</TooltipContent>
    </Tooltip>
  )
}
