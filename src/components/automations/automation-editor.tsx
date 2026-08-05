"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { ArrowLeft, Folder } from "lucide-react"
import { useTranslations } from "next-intl"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { AgentSelector } from "@/components/chat/agent-selector"
import {
  RichComposer,
  type RichComposerHandle,
} from "@/components/chat/composer/rich-composer"
import {
  useReferenceSearch,
  type ReferenceGroupLabels,
} from "@/components/chat/composer/use-reference-search"
import { docToPromptBlocks } from "@/components/chat/composer/to-prompt-blocks"
import { isComposerChromeClick } from "@/components/chat/composer/composer-commands"
import type { MentionUiLabels } from "@/components/chat/composer/suggestion/types"
import {
  AgentConfigSection,
  effectiveSelections,
  snapshotLabels,
} from "./agent-config-section"
import { automaticAgentMode } from "./automatic-agent-mode"
import {
  ComposerInvocationsPopup,
  useComposerInvocations,
} from "./composer-invocations"
import { SchedulePicker } from "./schedule-picker"
import { useAgentOptions } from "./use-agent-options"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { automationComputeNextRun } from "@/lib/api"
import { AGENT_LABELS } from "@/lib/types"
import type {
  AgentType,
  Automation,
  AutomationDraft,
  PromptInputBlock,
} from "@/lib/types"

interface AutomationEditorProps {
  /** The automation being edited, a template-seeded draft, or `null` for a
   *  blank create. Every field the editor reads is shared by `Automation` and
   *  `AutomationDraft`, so the `??` init chains seed from either. */
  automation: Automation | AutomationDraft | null
  onSubmit: (draft: AutomationDraft) => Promise<void>
  onCancel: () => void
  /** When present (the create-from-gallery flow), renders a "← Templates" link
   *  back to the picker. */
  onBackToTemplates?: () => void
}

function detectTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC"
  } catch {
    return "UTC"
  }
}

export function AutomationEditor({
  automation,
  onSubmit,
  onCancel,
  onBackToTemplates,
}: AutomationEditorProps) {
  const t = useTranslations("Automations")
  // The @-mention panel chrome reuses the chat composer's existing keys.
  const tComposer = useTranslations("Folder.chat.messageInput")
  const folders = useAppWorkspaceStore((s) => s.folders)

  const [name, setName] = useState(automation?.name ?? "")
  const [agentType, setAgentType] = useState<AgentType>(
    automation?.agent_type ?? "claude_code"
  )
  // Mirrors the composer's plain text for live validation; the authoritative
  // value is read from the editor ref at submit (so a prefilled edit validates
  // even before the user types — defaultText applies without firing onChange).
  const [prompt, setPrompt] = useState(automation?.config?.display_text ?? "")
  const [folderId, setFolderId] = useState<number | null>(
    automation?.root_folder_id ?? folders[0]?.id ?? null
  )
  const [cron, setCron] = useState(automation?.cron ?? "0 9 * * 1-5")
  // Detected from this device once and shown read-only (Codex-style — no manual
  // override). Still feeds the next-run preview and the cron builder.
  const [timezone] = useState(automation?.timezone ?? detectTimezone())
  const [configValues, setConfigValues] = useState<Record<string, string>>(
    automation?.config?.config_values ?? {}
  )
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [nextRun, setNextRun] = useState<string | null>(null)

  const editorRef = useRef<RichComposerHandle>(null)
  // True once the user explicitly picks an agent. A system fallback (saved agent
  // unavailable on this device) updates the displayed type via onFallback but
  // leaves this false, so submit can persist the original agent instead of
  // silently swapping it (see the submit handler).
  const userChoseAgentRef = useRef(false)

  const folderPath = useMemo(
    () => folders.find((f) => f.id === folderId)?.path ?? null,
    [folders, folderId]
  )

  // A folder is selected but its path has not resolved yet (the folder list is
  // still hydrating, or the folder was removed). Keep saving disabled until the
  // editor's folder-scoped references and skills point at a concrete path.
  const folderPathResolving = folderId != null && folderPath == null

  const referenceGroupLabels = useMemo<ReferenceGroupLabels>(
    () => ({
      file: tComposer("mentionGroupFile"),
      agent: tComposer("mentionGroupAgent"),
      session: tComposer("mentionGroupSession"),
      commit: tComposer("mentionGroupCommit"),
      skill: tComposer("mentionGroupSkill"),
    }),
    [tComposer]
  )
  const mentionUiLabels = useMemo<MentionUiLabels>(
    () => ({
      empty: tComposer("mentionEmpty"),
      loading: tComposer("mentionLoading"),
      listbox: tComposer("mentionListLabel"),
      more: tComposer("mentionMore"),
      count: (count: number) => tComposer("mentionCount", { count }),
    }),
    [tComposer]
  )
  // Live data sources for the @ panel (files/agents/sessions/commits). All
  // transport-only — no live ACP session needed; just the folder path.
  const referenceSearch = useReferenceSearch({
    defaultPath: folderPath,
    enabled: true,
    labels: referenceGroupLabels,
  })

  // The product-owned catalog feeds the config selectors and any fixed slash
  // commands. `$` Codex skills load separately from the filesystem.
  const agentOptions = useAgentOptions(agentType, folderPath)
  const invocations = useComposerInvocations({
    editorRef,
    agentType,
    folderPath,
    availableCommands: agentOptions.snapshot?.available_commands ?? [],
  })

  // Authoritative "next run" preview — same backend evaluator the scheduler
  // uses, so the previewed time can never diverge from the actual fire.
  useEffect(() => {
    if (!cron.trim()) {
      setNextRun(null)
      return
    }
    let cancelled = false
    const handle = setTimeout(() => {
      automationComputeNextRun(cron.trim(), timezone)
        .then((r) => {
          if (!cancelled) setNextRun(r)
        })
        .catch(() => {
          if (!cancelled) setNextRun(null)
        })
    }, 300)
    return () => {
      cancelled = true
      clearTimeout(handle)
    }
  }, [cron, timezone])

  // Backfill the default folder once the workspace folders finish hydrating — a
  // new (or template-seeded) automation opened before they load would otherwise
  // keep folderId null and block submit on errorFolder. Guarding on
  // `automation?.root_folder_id == null` (rather than `!automation`) also covers
  // a template draft seeded with a null folder, while never overriding the
  // folder of an existing automation being edited (its folderId is non-null, so
  // the `folderId == null` guard already short-circuits).
  useEffect(() => {
    if (
      folderId == null &&
      automation?.root_folder_id == null &&
      folders.length > 0
    ) {
      setFolderId(folders[0].id)
    }
  }, [folders, folderId, automation])

  const submit = async () => {
    setError(null)
    const editor = editorRef.current?.getEditor()
    const displayText = (editorRef.current?.getText() ?? prompt).trim()
    if (!name.trim()) return setError(t("errorName"))
    if (!displayText) return setError(t("errorPrompt"))
    if (!cron.trim()) return setError(t("errorCron"))
    if (folderId == null) return setError(t("errorFolder"))
    // The Save button is disabled while the selected folder path resolves; this
    // is a race-safety net for a submit triggered during that transition.
    if (folderPathResolving) return

    const blocks: PromptInputBlock[] = editor
      ? docToPromptBlocks(editor)
      : [{ type: "text", text: displayText }]

    setSaving(true)
    try {
      // Resolve the local catalog and pin exactly what the inline config bar
      // shows. An untouched selector displays the catalog's current value (no
      // "inherit" here), so persist it instead of a future default.
      const snapshot = await agentOptions.ensure()
      const { config_values } = effectiveSelections(
        snapshot,
        null,
        configValues
      )
      // Capture friendly labels for the chosen agent/folder/mode/options so the
      // detail page renders names, not raw value ids — and keeps doing so if the
      // agent is later uninstalled or the folder removed.
      const folderName = folders.find((f) => f.id === folderId)?.name
      // If the saved agent is unavailable on this device, AgentSelector shows a
      // substitute via onFallback. That swap is display-only: persisting it would
      // silently change the automation's agent (and invalidate its per-agent
      // config) when the user only meant to edit, say, the name or schedule. So
      // unless the user explicitly chose another agent, keep the original agent
      // and its saved config — while still applying prompt/name/schedule edits.
      const fellBackToSubstitute =
        !userChoseAgentRef.current &&
        !!automation &&
        automation.agent_type !== agentType
      const persistedAgentType = fellBackToSubstitute
        ? automation.agent_type
        : agentType
      const automaticMode = automaticAgentMode(persistedAgentType)
      const label_snapshot = {
        agent_label: AGENT_LABELS[agentType] ?? agentType,
        ...(folderName ? { folder_label: folderName } : {}),
        ...snapshotLabels(snapshot, automaticMode?.id ?? null, config_values),
        ...(automaticMode ? { mode_label: automaticMode.name } : {}),
      }

      const draft: AutomationDraft = {
        name: name.trim(),
        // Enable/disable lives on the detail header + row menu now; preserve an
        // existing automation's state and default new ones to enabled.
        enabled: automation?.enabled ?? true,
        trigger_kind: "schedule",
        cron: cron.trim(),
        timezone,
        agent_type: persistedAgentType,
        root_folder_id: folderId,
        isolation: "shared_in_root",
        branch: null,
        is_remote_branch: false,
        config: fellBackToSubstitute
          ? {
              // Preserve the original agent's saved config verbatim; only the
              // user-editable prompt is refreshed.
              prompt_blocks: blocks,
              display_text: displayText,
              mode_id: automaticMode?.id ?? null,
              config_values: automation.config?.config_values ?? {},
              label_snapshot:
                automation.config?.label_snapshot ?? label_snapshot,
            }
          : {
              prompt_blocks: blocks,
              display_text: displayText,
              mode_id: automaticMode?.id ?? null,
              config_values,
              label_snapshot,
            },
      }
      await onSubmit(draft)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-1 py-1">
      {onBackToTemplates ? (
        <button
          type="button"
          onClick={onBackToTemplates}
          className="-ml-1 inline-flex w-fit items-center gap-1 text-xs text-muted-foreground transition-colors hover:text-foreground"
        >
          <ArrowLeft className="h-3.5 w-3.5" aria-hidden="true" />
          {t("backToTemplates")}
        </button>
      ) : null}

      {/* Name — borderless title input */}
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder={t("namePlaceholder")}
        aria-label={t("name")}
        className="w-full bg-transparent text-lg font-semibold tracking-tight outline-none placeholder:font-normal placeholder:text-muted-foreground/50"
      />

      {/* Agent pill — above the composer box, as on the new-conversation screen */}
      <div className="flex">
        <AgentSelector
          defaultAgentType={agentType}
          onSelect={(a) => {
            // Switching agents changes the option universe — reset overrides.
            userChoseAgentRef.current = true
            setAgentType(a)
            setConfigValues({})
          }}
          // A system substitution (saved agent unavailable) updates the type but
          // must NOT be treated as a user choice that wipes the saved config.
          onFallback={setAgentType}
        />
      </div>

      {/* The real conversation composer (rich text + @-mentions) plus an inline
          config bottom bar, matching the new-conversation input. */}
      <div
        // Clicking the box's blank chrome (padding, the dead space below a short
        // prompt, the config-bar gaps) focuses the editor at the click point —
        // same affordance as the chat composer. Interactive controls, badges and
        // the editor surface exclude themselves via NON_CHROME_SELECTOR;
        // `iyw-claw-composer-chrome` paints the text I-beam over the dead space.
        onMouseDown={(e) => {
          if (!isComposerChromeClick(e.target)) return
          e.preventDefault()
          editorRef.current?.focusAtCoords(e.clientX, e.clientY)
        }}
        className="iyw-claw-composer-chrome relative rounded-xl border border-input bg-background transition-colors focus-within:border-ring focus-within:ring-[3px] focus-within:ring-inset focus-within:ring-ring/50"
      >
        <ComposerInvocationsPopup inv={invocations} />
        <RichComposer
          ref={editorRef}
          defaultText={automation?.config?.display_text ?? ""}
          placeholder={t("promptPlaceholder")}
          ariaLabel={t("prompt")}
          referenceSearch={referenceSearch}
          mentionUiLabels={mentionUiLabels}
          tabLabels={referenceGroupLabels}
          onChange={(text) => {
            setPrompt(text)
            invocations.detect()
          }}
          isExternalMenuOpen={invocations.isOpen}
          onExternalMenuKeyDown={invocations.onKeyDown}
          className="max-h-[18rem] min-h-[7.5rem]"
        />
        <div className="px-2 pb-2 pt-1">
          <AgentConfigSection
            snapshot={agentOptions.snapshot}
            loading={agentOptions.loading}
            error={agentOptions.error}
            onReload={agentOptions.reload}
            modeId={automaticAgentMode(agentType)?.id ?? null}
            configValues={configValues}
            layout="inline"
            hideMode
            onModeChange={() => undefined}
            onConfigChange={(optionId, valueId) =>
              setConfigValues((prev) => {
                const next = { ...prev }
                if (valueId === null) delete next[optionId]
                else next[optionId] = valueId
                return next
              })
            }
          />
        </div>
      </div>

      {/* Target — automated runs always use the selected workspace folder. */}
      <div className="flex flex-col gap-2">
        <h3 className="text-[0.6875rem] font-medium uppercase tracking-wide text-muted-foreground">
          {t("sectionTarget")}
        </h3>
        <div className="flex flex-wrap items-center gap-2">
          <Select
            value={folderId != null ? String(folderId) : undefined}
            onValueChange={(v) => setFolderId(Number(v))}
          >
            <SelectTrigger size="sm" className="h-7 gap-1.5 text-xs">
              <Folder
                className="size-3.5 text-muted-foreground"
                aria-hidden="true"
              />
              <SelectValue placeholder={t("folderPlaceholder")} />
            </SelectTrigger>
            <SelectContent>
              {folders.map((f) => (
                <SelectItem key={f.id} value={String(f.id)}>
                  {f.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        {/* Running in the folder shares the user's working tree (and any
            concurrent run); surface that trade-off near the target controls. */}
        <p className="text-xs text-muted-foreground">
          {t("isolationSharedCaveat")}
        </p>
      </div>

      {/* Cron remains the persisted format; users edit a structured cadence. */}
      <div className="flex flex-col gap-2">
        <h3 className="text-[0.6875rem] font-medium uppercase tracking-wide text-muted-foreground">
          {t("trigger")}
        </h3>
        <SchedulePicker
          initialCron={cron}
          timezone={timezone}
          nextRun={nextRun}
          onChange={setCron}
        />
      </div>

      {error ? (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      ) : null}

      <div className="mt-1 flex justify-end gap-2">
        <Button
          type="button"
          variant="ghost"
          onClick={onCancel}
          disabled={saving}
        >
          {t("cancel")}
        </Button>
        <Button
          type="button"
          onClick={submit}
          disabled={saving || folderPathResolving}
        >
          {t("save")}
        </Button>
      </div>
    </div>
  )
}
