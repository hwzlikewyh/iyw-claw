"use client"

import { useState, type ReactNode } from "react"
import {
  Check,
  Loader2,
  PackageCheck,
  Sparkles,
  Store,
  Upload,
  WandSparkles,
  type LucideIcon,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { cn } from "@/lib/utils"

export type SkillMarketTab = "installed" | "official" | "import" | "generate"

type MarketTranslator = (
  key: string,
  values?: Record<string, string | number>
) => string

export interface SkillContentRequest {
  id: string
  content: string
}

interface OfficialSkillTemplate {
  id: string
  tags: string[]
}

const OFFICIAL_SKILL_TEMPLATES: OfficialSkillTemplate[] = [
  { id: "code-review", tags: ["review", "quality"] },
  { id: "test-writer", tags: ["test", "vitest"] },
  { id: "docs-polish", tags: ["docs", "writing"] },
  { id: "ui-ux-review", tags: ["frontend", "design"] },
]

function useMarketTranslations(): MarketTranslator {
  return useTranslations("SkillsSettings.market") as unknown as MarketTranslator
}

function normalizeSkillId(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_.-]+/g, "-")
    .replace(/^-+|-+$/g, "")
}

function yamlString(value: string): string {
  return JSON.stringify(value.trim())
}

function buildGeneratedSkillContent(params: {
  id: string
  title: string
  description: string
  instructions: string
}): string {
  const title = params.title.trim() || params.id
  const description = params.description.trim()
  const instructions = params.instructions.trim()

  return [
    "---",
    `name: ${params.id}`,
    `description: ${yamlString(description || title)}`,
    "---",
    "",
    `# ${title}`,
    "",
    "## When to use",
    "",
    description || "Describe when this skill should be used.",
    "",
    "## Instructions",
    "",
    instructions || "1. Add actionable instruction one.",
    "",
  ].join("\n")
}

function PanelShell({
  icon: Icon,
  title,
  description,
  targetName,
  children,
}: {
  icon: LucideIcon
  title: string
  description: string
  targetName: string | null
  children: ReactNode
}) {
  const t = useMarketTranslations()

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-5 p-4 sm:p-6">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex min-w-0 gap-3">
            <span className="flex size-10 shrink-0 items-center justify-center rounded-xl border border-border bg-card text-muted-foreground">
              <Icon className="size-5" aria-hidden="true" />
            </span>
            <div className="min-w-0">
              <h2 className="text-base font-semibold">{title}</h2>
              <p className="mt-1 max-w-2xl text-sm text-muted-foreground">
                {description}
              </p>
            </div>
          </div>
          <Badge
            variant="outline"
            className={cn(
              "w-fit shrink-0",
              targetName
                ? "border-primary/30 bg-primary/5 text-primary"
                : "text-muted-foreground"
            )}
          >
            {targetName ? t("target", { target: targetName }) : t("noTarget")}
          </Badge>
        </div>
        {children}
      </div>
    </div>
  )
}

export function OfficialSkillMarketPanel({
  targetName,
  installedIds,
  disabled,
  busyKey,
  onInstall,
}: {
  targetName: string | null
  installedIds: Set<string>
  disabled: boolean
  busyKey: string | null
  onInstall: (request: SkillContentRequest) => void
}) {
  const t = useMarketTranslations()

  return (
    <PanelShell
      icon={Store}
      title={t("official.title")}
      description={t("official.description")}
      targetName={targetName}
    >
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
        {OFFICIAL_SKILL_TEMPLATES.map((template) => {
          const isInstalled = installedIds.has(template.id)
          const key = `official:${template.id}`
          const isBusy = busyKey === key

          return (
            <article
              key={template.id}
              className="flex min-h-[15rem] flex-col rounded-xl border border-border bg-card p-4"
            >
              <div className="flex items-start justify-between gap-2">
                <PackageCheck
                  className="size-5 shrink-0 text-primary"
                  aria-hidden="true"
                />
                {isInstalled ? (
                  <Badge variant="outline" className="text-[0.6875rem]">
                    {t("official.installedBadge")}
                  </Badge>
                ) : null}
              </div>
              <h3 className="mt-3 text-sm font-semibold">
                {t(`official.items.${template.id}.title`)}
              </h3>
              <p className="mt-2 flex-1 text-xs leading-5 text-muted-foreground">
                {t(`official.items.${template.id}.description`)}
              </p>
              <div className="mt-3 flex flex-wrap gap-1.5">
                {template.tags.map((tag) => (
                  <Badge
                    key={tag}
                    variant="outline"
                    className="text-[0.625rem] text-muted-foreground"
                  >
                    {tag}
                  </Badge>
                ))}
              </div>
              <Button
                size="sm"
                variant={isInstalled ? "outline" : "default"}
                className="mt-4 w-full"
                disabled={disabled || (busyKey != null && !isBusy)}
                onClick={() =>
                  onInstall({
                    id: template.id,
                    content: t(`official.items.${template.id}.content`),
                  })
                }
              >
                {isBusy ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : isInstalled ? (
                  <Check className="size-3.5" />
                ) : (
                  <Sparkles className="size-3.5" />
                )}
                {isInstalled
                  ? t("official.viewInstalled")
                  : t("official.install")}
              </Button>
            </article>
          )
        })}
      </div>
    </PanelShell>
  )
}

export function ImportSkillPanel({
  targetName,
  disabled,
  busy,
  onImport,
}: {
  targetName: string | null
  disabled: boolean
  busy: boolean
  onImport: (request: SkillContentRequest) => void
}) {
  const t = useMarketTranslations()
  const [skillId, setSkillId] = useState("")
  const [content, setContent] = useState("")
  const normalizedId = normalizeSkillId(skillId)
  const canImport = !disabled && normalizedId && content.trim() && !busy

  return (
    <PanelShell
      icon={Upload}
      title={t("import.title")}
      description={t("import.description")}
      targetName={targetName}
    >
      <div className="grid gap-4 lg:grid-cols-[18rem_1fr]">
        <section className="rounded-xl border border-border bg-card p-4">
          <h3 className="text-sm font-semibold">{t("import.formTitle")}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t("import.formDescription")}
          </p>
          <div className="mt-4 space-y-3">
            <Input
              value={skillId}
              onChange={(event) => setSkillId(event.target.value)}
              placeholder={t("import.idPlaceholder")}
            />
            {skillId.trim() ? (
              <p className="text-[0.6875rem] text-muted-foreground">
                {t("import.normalizedId", {
                  id: normalizedId || t("import.invalidId"),
                })}
              </p>
            ) : null}
            <Button
              className="w-full"
              disabled={!canImport}
              onClick={() => onImport({ id: normalizedId, content })}
            >
              {busy ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Upload className="size-3.5" />
              )}
              {t("import.submit")}
            </Button>
          </div>
        </section>
        <Textarea
          value={content}
          onChange={(event) => setContent(event.target.value)}
          placeholder={t("import.contentPlaceholder")}
          className="min-h-[24rem] resize-none font-mono text-xs"
        />
      </div>
    </PanelShell>
  )
}

export function GenerateSkillPanel({
  targetName,
  disabled,
  busy,
  onGenerate,
}: {
  targetName: string | null
  disabled: boolean
  busy: boolean
  onGenerate: (request: SkillContentRequest) => void
}) {
  const t = useMarketTranslations()
  const [skillId, setSkillId] = useState("")
  const [title, setTitle] = useState("")
  const [description, setDescription] = useState("")
  const [instructions, setInstructions] = useState("")
  const normalizedId = normalizeSkillId(skillId || title)
  const canGenerate = !disabled && normalizedId && description.trim() && !busy

  return (
    <PanelShell
      icon={WandSparkles}
      title={t("generate.title")}
      description={t("generate.description")}
      targetName={targetName}
    >
      <div className="grid gap-4 lg:grid-cols-[22rem_1fr]">
        <section className="rounded-xl border border-border bg-card p-4">
          <h3 className="text-sm font-semibold">{t("generate.formTitle")}</h3>
          <div className="mt-4 space-y-3">
            <Input
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder={t("generate.titlePlaceholder")}
            />
            <Input
              value={skillId}
              onChange={(event) => setSkillId(event.target.value)}
              placeholder={t("generate.idPlaceholder")}
            />
            <Textarea
              value={description}
              onChange={(event) => setDescription(event.target.value)}
              placeholder={t("generate.descriptionPlaceholder")}
              className="min-h-24 resize-none text-xs"
            />
            <Textarea
              value={instructions}
              onChange={(event) => setInstructions(event.target.value)}
              placeholder={t("generate.instructionsPlaceholder")}
              className="min-h-32 resize-none text-xs"
            />
            <Button
              className="w-full"
              disabled={!canGenerate}
              onClick={() =>
                onGenerate({
                  id: normalizedId,
                  content: buildGeneratedSkillContent({
                    id: normalizedId,
                    title,
                    description,
                    instructions,
                  }),
                })
              }
            >
              {busy ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <WandSparkles className="size-3.5" />
              )}
              {t("generate.submit")}
            </Button>
          </div>
        </section>
        <section className="rounded-xl border border-border bg-muted/15 p-4">
          <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
            <Sparkles className="size-3.5" aria-hidden="true" />
            {t("generate.previewTitle")}
          </div>
          <pre className="mt-3 max-h-[28rem] overflow-auto whitespace-pre-wrap rounded-lg border bg-background p-3 font-mono text-xs leading-5">
            {buildGeneratedSkillContent({
              id: normalizedId || "my-skill",
              title,
              description,
              instructions,
            })}
          </pre>
        </section>
      </div>
    </PanelShell>
  )
}
