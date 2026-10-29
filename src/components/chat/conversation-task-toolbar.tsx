import type { ReactNode } from "react"
import { MessageCircleQuestionMark } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"

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
      {onSideQuestion && <SideQuestionButton onAsk={onSideQuestion} />}
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
    <Button
      variant="ghost"
      size="sm"
      className="conversation-side-question"
      onClick={() => onAsk("")}
    >
      <MessageCircleQuestionMark className="size-4" />
      {t("title")}
    </Button>
  )
}
