"use client"

import {
  BarChart3,
  CalendarCheck,
  FileSearch,
  ListChecks,
  PenLine,
  Search,
  type LucideIcon,
} from "lucide-react"

import { cn } from "@/lib/utils"
import type { ComposerInjectContent } from "@/components/chat/message-input"

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
    title: "制定行动计划",
    description: "明确目标、优先级和下一步",
    prompt:
      "请帮我制定一份清晰可执行的行动计划，先明确目标和限制，再按优先级拆分步骤，并标出下一步该做什么。",
    icon: CalendarCheck,
    tone: "amber",
  },
  {
    id: "write",
    title: "撰写内容草稿",
    description: "起草邮件、方案或宣传文案",
    prompt:
      "请根据我接下来提供的背景和要求撰写内容草稿，结构清楚、表达自然，并匹配目标读者和使用场景。",
    icon: PenLine,
    tone: "blue",
  },
  {
    id: "summarize",
    title: "总结资料要点",
    description: "提炼结论、重点和待办事项",
    prompt:
      "请帮我阅读并总结接下来提供的资料，提炼核心观点、重要事实、关键结论和需要跟进的事项。",
    icon: FileSearch,
    tone: "pink",
  },
  {
    id: "analyze",
    title: "分析数据问题",
    description: "发现趋势、异常和可能原因",
    prompt:
      "请分析我接下来提供的数据或现象，找出主要趋势、异常点和可能原因，并给出有依据的结论与建议。",
    icon: BarChart3,
    tone: "green",
  },
  {
    id: "meeting",
    title: "整理会议记录",
    description: "归纳决策、负责人和截止时间",
    prompt:
      "请帮我整理接下来提供的会议记录，归纳讨论结论、已做决策、待办事项、负责人和截止时间。",
    icon: ListChecks,
    tone: "purple",
  },
  {
    id: "research",
    title: "调研比较选项",
    description: "收集信息并比较优缺点",
    prompt:
      "请围绕我接下来提供的主题进行调研，比较主要选项的优缺点、适用条件和风险，并给出推荐结论。",
    icon: Search,
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
}

export function QuickActions({ onSelect }: QuickActionsProps) {
  return (
    <section className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
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
    </section>
  )
}
