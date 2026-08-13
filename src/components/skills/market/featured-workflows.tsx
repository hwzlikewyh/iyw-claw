"use client"

import Image from "next/image"
import {
  ArrowUpRight,
  BookOpenCheck,
  Boxes,
  Bug,
  CheckCircle2,
  Code2,
  FileSearch,
  GitPullRequest,
  Rocket,
  ShieldCheck,
  Sparkles,
  TestTube2,
  Workflow,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { ScrollArea } from "@/components/ui/scroll-area"
import { cn } from "@/lib/utils"

const WORKFLOWS = [
  {
    id: "quality",
    category: "it-ops-security",
    image: "/capability-market/workflow-quality.jpg",
    icon: ShieldCheck,
    stepIcons: [FileSearch, Bug, CheckCircle2],
    tone: "border-blue-200/70 bg-blue-50/85 dark:border-blue-900/50 dark:bg-blue-950/65",
    iconTone: "bg-blue-600 text-white",
  },
  {
    id: "frontend",
    category: "design-media",
    image: "/capability-market/workflow-frontend.jpg",
    icon: Code2,
    stepIcons: [Sparkles, Code2, TestTube2],
    tone: "border-emerald-200/70 bg-emerald-50/85 dark:border-emerald-900/50 dark:bg-emerald-950/65",
    iconTone: "bg-emerald-700 text-white",
  },
  {
    id: "knowledge",
    category: "knowledge-management",
    image: "/capability-market/workflow-knowledge.jpg",
    icon: BookOpenCheck,
    stepIcons: [FileSearch, Boxes, BookOpenCheck],
    tone: "border-amber-200/70 bg-amber-50/85 dark:border-amber-900/50 dark:bg-amber-950/65",
    iconTone: "bg-amber-700 text-white",
  },
  {
    id: "delivery",
    category: "dev-programming",
    image: "/capability-market/workflow-delivery.jpg",
    icon: Rocket,
    stepIcons: [GitPullRequest, Workflow, Rocket],
    tone: "border-rose-200/70 bg-rose-50/85 dark:border-rose-900/50 dark:bg-rose-950/65",
    iconTone: "bg-rose-700 text-white",
  },
] as const

const STEP_KEYS = ["step1", "step2", "step3"] as const

export function FeaturedWorkflows({
  selectedCategory,
  onSelectCategory,
}: {
  selectedCategory: string | null
  onSelectCategory: (category: string) => void
}) {
  const t = useTranslations("CapabilityMarket.featured")

  return (
    <section className="shrink-0 border-b bg-muted/[0.12] px-4 py-5 sm:px-6">
      <div className="mb-3.5">
        <div>
          <h2 className="text-base font-semibold">{t("title")}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t("subtitle")}</p>
        </div>
      </div>

      <ScrollArea x="scroll" y="hidden" className="w-full">
        <div className="grid min-w-[920px] grid-cols-[1.55fr_repeat(3,minmax(0,1fr))] gap-3 pb-1 xl:min-w-0">
          {WORKFLOWS.map((workflow, index) => (
            <WorkflowCard
              key={workflow.id}
              workflow={workflow}
              featured={index === 0}
              selected={workflow.category === selectedCategory}
              onSelectCategory={onSelectCategory}
            />
          ))}
        </div>
      </ScrollArea>
    </section>
  )
}

function WorkflowCard({
  workflow,
  featured,
  selected,
  onSelectCategory,
}: {
  workflow: (typeof WORKFLOWS)[number]
  featured: boolean
  selected: boolean
  onSelectCategory: (category: string) => void
}) {
  const t = useTranslations("CapabilityMarket.featured")
  const Icon = workflow.icon
  return (
    <button
      type="button"
      className={`group relative min-h-40 overflow-hidden rounded-lg border text-left outline-none transition-[border-color,box-shadow,transform] hover:-translate-y-0.5 hover:border-foreground/20 hover:shadow-md focus-visible:ring-2 focus-visible:ring-ring/50 ${selected ? "ring-1 ring-foreground/35" : ""} ${workflow.tone}`}
      onClick={() => onSelectCategory(workflow.category)}
      aria-label={t("openCategory", { name: t(`${workflow.id}.title`) })}
    >
      <Image
        src={workflow.image}
        alt=""
        fill
        priority={featured}
        sizes="(min-width: 1280px) 22vw, 200px"
        className={
          featured
            ? "object-cover opacity-45 saturate-75"
            : "object-cover opacity-[0.14] saturate-50"
        }
      />
      {featured ? (
        <span
          className="absolute inset-y-0 left-0 w-[61%] bg-blue-50/95 dark:bg-blue-950/95"
          aria-hidden="true"
        />
      ) : null}
      <div className="relative z-10 flex h-full min-h-40 flex-col p-4">
        <div
          className={`flex items-start gap-2.5 ${featured ? "w-[64%]" : "w-full"}`}
        >
          <span
            className={`flex size-7 items-center justify-center rounded-md ${workflow.iconTone}`}
          >
            <Icon className="size-3.5" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <p className="text-[10px] font-semibold uppercase text-muted-foreground">
              {t(`${workflow.id}.kicker`)}
            </p>
            <h3 className="mt-0.5 text-sm font-semibold leading-5">
              {t(`${workflow.id}.title`)}
            </h3>
          </div>
          <ArrowUpRight
            className="ml-auto size-3.5 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5 group-hover:-translate-y-0.5"
            aria-hidden="true"
          />
        </div>
        <WorkflowSteps workflow={workflow} featured={featured} />
      </div>
    </button>
  )
}

function WorkflowSteps({
  workflow,
  featured,
}: {
  workflow: (typeof WORKFLOWS)[number]
  featured: boolean
}) {
  const t = useTranslations("CapabilityMarket.featured")
  return (
    <div className="mt-auto flex items-center gap-1.5 pt-5">
      {workflow.stepIcons.map((StepIcon, index) => (
        <div key={index} className="flex min-w-0 flex-1 items-center gap-1.5">
          <span className="flex size-6 shrink-0 items-center justify-center rounded-md border border-black/10 bg-white/85 text-foreground dark:border-white/10 dark:bg-black/25">
            <StepIcon className="size-3" aria-hidden="true" />
          </span>
          <span
            className={cn(
              "truncate text-[10px] font-medium",
              !featured && "hidden xl:inline"
            )}
          >
            {t(`${workflow.id}.steps.${STEP_KEYS[index] ?? "step1"}`)}
          </span>
          {index < workflow.stepIcons.length - 1 ? (
            <span
              className="ml-auto h-px w-2 shrink-0 bg-foreground/20"
              aria-hidden="true"
            />
          ) : null}
        </div>
      ))}
    </div>
  )
}
