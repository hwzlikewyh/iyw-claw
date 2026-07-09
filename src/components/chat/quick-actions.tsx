"use client"

import {
  Bug,
  Code2,
  FileText,
  GitBranch,
  Lightbulb,
  ListChecks,
  type LucideIcon,
} from "lucide-react"

import { cn } from "@/lib/utils"
import type { ComposerInjectContent } from "@/components/chat/message-input"
import type { AgentType } from "@/lib/types"

interface SuggestedPrompt {
  id: string
  title: string
  description: string
  prompt: string
  icon: LucideIcon
  tone: "amber" | "blue" | "pink" | "green" | "purple" | "slate"
}

const SUGGESTED_PROMPTS: SuggestedPrompt[] = [
  {
    id: "plan",
    title: "梳理实现方案",
    description: "先明确目标、边界和验证方式",
    prompt:
      "请先帮我梳理这个需求的实现方案，明确目标、改动边界、风险点和验证方式。",
    icon: Lightbulb,
    tone: "amber",
  },
  {
    id: "implement",
    title: "开始代码开发",
    description: "按现有代码风格完成改动",
    prompt: "请基于当前仓库实现这个需求，遵循现有代码风格，并完成必要验证。",
    icon: Code2,
    tone: "blue",
  },
  {
    id: "debug",
    title: "排查并修复问题",
    description: "先定位根因，再做最小修复",
    prompt: "请帮我排查这个问题，先定位根因，再给出最小修复并验证结果。",
    icon: Bug,
    tone: "pink",
  },
  {
    id: "review",
    title: "检查当前改动",
    description: "重点看风险、回归和缺失验证",
    prompt:
      "请 review 当前改动，优先指出 bug、行为回归、风险点和缺失的测试验证。",
    icon: ListChecks,
    tone: "green",
  },
  {
    id: "docs",
    title: "整理文档说明",
    description: "把实现和使用方式写清楚",
    prompt: "请帮我整理这块功能的文档说明，包括使用方式、关键配置和注意事项。",
    icon: FileText,
    tone: "purple",
  },
  {
    id: "git",
    title: "处理 Git 变更",
    description: "查看状态、归纳改动和准备提交",
    prompt: "请帮我查看当前 Git 变更，归纳改动内容，并给出合适的提交说明。",
    icon: GitBranch,
    tone: "slate",
  },
]

const TONE_CLASSES: Record<SuggestedPrompt["tone"], string> = {
  amber:
    "border-amber-500/20 hover:border-amber-500/40 hover:bg-amber-500/5 text-amber-600 dark:text-amber-400",
  blue: "border-blue-500/20 hover:border-blue-500/40 hover:bg-blue-500/5 text-blue-600 dark:text-blue-400",
  pink: "border-pink-500/20 hover:border-pink-500/40 hover:bg-pink-500/5 text-pink-600 dark:text-pink-400",
  green:
    "border-green-500/20 hover:border-green-500/40 hover:bg-green-500/5 text-green-600 dark:text-green-400",
  purple:
    "border-purple-500/20 hover:border-purple-500/40 hover:bg-purple-500/5 text-purple-600 dark:text-purple-400",
  slate:
    "border-slate-500/20 hover:border-slate-500/40 hover:bg-slate-500/5 text-slate-600 dark:text-slate-300",
}

interface QuickActionsProps {
  onSelect: (payload: ComposerInjectContent) => void
  agentType: AgentType | null
}

export function QuickActions({ onSelect }: QuickActionsProps) {
  return (
    <section className="space-y-2">
      <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
        {SUGGESTED_PROMPTS.map((item) => {
          const Icon = item.icon
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onSelect({ text: item.prompt })}
              className={cn(
                "group flex min-h-20 flex-col items-start gap-1.5 rounded-lg border bg-card/50 px-3 py-2.5 text-left transition-colors",
                TONE_CLASSES[item.tone]
              )}
            >
              <Icon aria-hidden className="h-4 w-4 shrink-0" />
              <span className="text-sm font-medium text-foreground">
                {item.title}
              </span>
              <span className="line-clamp-1 text-xs text-muted-foreground">
                {item.description}
              </span>
            </button>
          )
        })}
      </div>
    </section>
  )
}
