import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"

import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import type { SkillToggleItem } from "./skill-toggle-list-model"

interface SkillToggleDetailSheetProps {
  skill: SkillToggleItem | null
  content: string
  loading: boolean
  onClose: () => void
}

export function SkillToggleDetailSheet({
  skill,
  content,
  loading,
  onClose,
}: SkillToggleDetailSheetProps) {
  const t = useTranslations("SkillMatrix")
  return (
    <Sheet
      open={skill !== null}
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <SheetContent className="flex w-[min(680px,100vw)] flex-col sm:max-w-[680px]">
        {skill ? (
          <>
            <SheetHeader>
              <SheetTitle>{skill.displayName}</SheetTitle>
              <SheetDescription>{skill.description}</SheetDescription>
            </SheetHeader>
            <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
              <div className="mb-2 text-xs text-muted-foreground">
                {t("detail.preview")}
              </div>
              {loading ? (
                <div className="flex items-center text-sm text-muted-foreground">
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  {t("detail.loadingContent")}
                </div>
              ) : (
                <div className="prose prose-sm dark:prose-invert max-w-none">
                  <ReactMarkdown remarkPlugins={[remarkGfm]}>
                    {content}
                  </ReactMarkdown>
                </div>
              )}
            </div>
          </>
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
