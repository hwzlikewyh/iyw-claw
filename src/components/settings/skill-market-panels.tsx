"use client"

import { useState, type ReactNode } from "react"
import {
  Loader2,
  Sparkles,
  Upload,
  WandSparkles,
  type LucideIcon,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { SkillMarketFolderPicker } from "@/components/skills/skill-market-folder-picker"
import type { SelectedSkillMarketFolder } from "@/lib/skill-market"
import type { AgentSkillFile } from "@/lib/types"
import { cn } from "@/lib/utils"

type MarketTranslator = (
  key: string,
  values?: Record<string, string | number>
) => string

export interface SkillContentRequest {
  id: string
  content: string
  files?: AgentSkillFile[]
}

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

function bytesToBase64(bytes: Uint8Array): string {
  let binary = ""
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000))
  }
  return btoa(binary)
}

function decodeBase64(value: string): string {
  const binary = atob(value)
  return new TextDecoder().decode(
    Uint8Array.from(binary, (character) => character.charCodeAt(0))
  )
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
    `description: ${JSON.stringify(description || title)}`,
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
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-5 p-4 sm:p-6">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex min-w-0 gap-3">
            <span className="flex size-10 shrink-0 items-center justify-center rounded-lg border bg-card text-muted-foreground">
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
            className={cn("w-fit", targetName && "text-primary")}
          >
            {targetName ? t("target", { target: targetName }) : t("noTarget")}
          </Badge>
        </div>
        {children}
      </div>
    </div>
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
  const [folder, setFolder] = useState<SelectedSkillMarketFolder | null>(null)
  const normalizedId = normalizeSkillId(skillId)
  const canImport = !disabled && normalizedId && content.trim() && !busy
  const selectFolder = (value: SelectedSkillMarketFolder | null) => {
    setFolder(value)
    if (!value) return
    setSkillId(normalizeSkillId(value.name))
    const skillFile = value.files.find((file) => file.path === "SKILL.md")
    if (skillFile) setContent(decodeBase64(skillFile.contentBase64))
  }
  const submit = () => {
    const files = folder?.files.map((file) => ({
      path: file.path,
      contentBase64:
        file.path === "SKILL.md"
          ? bytesToBase64(new TextEncoder().encode(content))
          : file.contentBase64,
    }))
    onImport({ id: normalizedId, content, files })
  }
  return (
    <PanelShell
      icon={Upload}
      title={t("import.title")}
      description={t("import.description")}
      targetName={targetName}
    >
      <div className="grid gap-4 lg:grid-cols-[18rem_1fr]">
        <section className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-semibold">{t("import.formTitle")}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t("import.formDescription")}
          </p>
          <div className="mt-4 space-y-3">
            <SkillMarketFolderPicker
              folder={folder}
              disabled={disabled || busy}
              onChange={selectFolder}
            />
            <Input
              value={skillId}
              onChange={(event) => setSkillId(event.target.value)}
              placeholder={t("import.idPlaceholder")}
            />
            {skillId.trim() ? (
              <p className="text-[11px] text-muted-foreground">
                {t("import.normalizedId", {
                  id: normalizedId || t("import.invalidId"),
                })}
              </p>
            ) : null}
            <Button className="w-full" disabled={!canImport} onClick={submit}>
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
  const content = buildGeneratedSkillContent({
    id: normalizedId || "my-skill",
    title,
    description,
    instructions,
  })
  return (
    <PanelShell
      icon={WandSparkles}
      title={t("generate.title")}
      description={t("generate.description")}
      targetName={targetName}
    >
      <div className="grid gap-4 lg:grid-cols-[22rem_1fr]">
        <section className="rounded-lg border bg-card p-4">
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
              disabled={
                disabled || !normalizedId || !description.trim() || busy
              }
              onClick={() => onGenerate({ id: normalizedId, content })}
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
        <section className="rounded-lg border bg-muted/15 p-4">
          <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
            <Sparkles className="size-3.5" />
            {t("generate.previewTitle")}
          </div>
          <pre className="mt-3 max-h-[28rem] overflow-auto whitespace-pre-wrap rounded-lg border bg-background p-3 font-mono text-xs leading-5">
            {content}
          </pre>
        </section>
      </div>
    </PanelShell>
  )
}
