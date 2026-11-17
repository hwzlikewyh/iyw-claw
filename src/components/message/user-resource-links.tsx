"use client"

import { BookOpenText, FileSearch } from "lucide-react"
import type { UserResourceDisplay } from "@/lib/adapters/ai-elements-adapter"
import { knowledgeSourceKind } from "@/lib/knowledge-source-display"
import { cn } from "@/lib/utils"

interface UserResourceLinksProps {
  resources: UserResourceDisplay[]
  className?: string
}

/** 用户消息的附件块；知识库引用只显示来源名称，内部地址保留在消息数据中。 */
export function UserResourceLinks({
  resources,
  className,
}: UserResourceLinksProps) {
  if (resources.length === 0) return null

  return (
    <div className={className}>
      <div className="flex max-w-full flex-wrap gap-1.5">
        {resources.map((resource, index) => {
          const knowledge = knowledgeSourceKind(resource.uri) !== null
          const Icon = knowledge ? BookOpenText : FileSearch
          return (
            <div
              key={`${resource.uri}-${index}`}
              data-knowledge-reference={knowledge || undefined}
              title={resource.name}
              className={cn(
                "inline-flex min-w-0 max-w-full items-center gap-1 border px-2 py-1 text-xs",
                knowledge
                  ? "gap-1.5 rounded-md border-violet-500/25 bg-violet-500/10 font-medium text-violet-700 dark:text-violet-300"
                  : "rounded-full border-border/70 bg-muted/40 text-muted-foreground"
              )}
            >
              <Icon
                aria-hidden="true"
                className={cn("shrink-0", knowledge ? "size-3.5" : "size-3")}
              />
              <span className="max-w-56 truncate">{resource.name}</span>
            </div>
          )
        })}
      </div>
    </div>
  )
}
