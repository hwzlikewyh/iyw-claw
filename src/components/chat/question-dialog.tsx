"use client"

import { useEffect, useRef } from "react"
import { useTranslations } from "next-intl"
import { matchShortcutEvent } from "@/lib/keyboard-shortcuts"
import { useShortcutSettings } from "@/hooks/use-shortcut-settings"
import type { PendingQuestion } from "@/contexts/acp-connections-context"
import { AskQuestionCard } from "./ask-question-card"

interface QuestionDialogProps {
  question: PendingQuestion | null
  onAnswer: (answer: string) => void | Promise<void>
}

export function QuestionDialog({ question, onAnswer }: QuestionDialogProps) {
  const t = useTranslations("Folder.chat.questionDialog")
  const { shortcuts } = useShortcutSettings()
  const container = useRef<HTMLDivElement>(null)
  const id = question?.tool_call_id
  useEffect(() => {
    container.current?.querySelector("textarea")?.focus()
  }, [id])
  if (!question) return null
  const pending = {
    question_id: question.tool_call_id,
    created_at: "",
    questions: [
      {
        id: question.tool_call_id,
        question: question.question,
        header: t("title"),
        options: [],
        multi_select: false,
      },
    ],
  }
  return (
    <div
      ref={container}
      className="mx-4 mb-1 min-w-0 shrink-0"
      onKeyDown={(event) => {
        if (
          event.target instanceof HTMLTextAreaElement &&
          matchShortcutEvent(event, shortcuts.send_message)
        ) {
          event.preventDefault()
          container.current
            ?.querySelector<HTMLButtonElement>("[data-question-submit]")
            ?.click()
        }
      }}
    >
      <AskQuestionCard
        question={pending}
        title={t("title")}
        allowSkip={false}
        onAnswer={async (_, answer) => {
          const text = answer.answers[0]?.labels[0]
          if (text) await onAnswer(text)
        }}
      />
    </div>
  )
}
