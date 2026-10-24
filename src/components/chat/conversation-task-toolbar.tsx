import type { ReactNode } from "react"
import { MessageCircleQuestionMark } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"

export function ConversationTaskToolbar({
  task,
  onSideQuestion,
}: {
  task?: ReactNode
  onSideQuestion?: (question: string) => boolean
}) {
  return (
    <div className="conversation-task-toolbar">
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
