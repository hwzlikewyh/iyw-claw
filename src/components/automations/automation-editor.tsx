"use client"
import { ArrowLeft, Folder } from "lucide-react"
import { AgentSelector } from "@/components/chat/agent-selector"
import { RichComposer } from "@/components/chat/composer/rich-composer"
import { isComposerChromeClick } from "@/components/chat/composer/composer-commands"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { AgentConfigSection } from "./agent-config-section"
import { automaticAgentMode } from "./automatic-agent-mode"
import { ComposerInvocationsPopup } from "./composer-invocations"
import { SchedulePicker } from "./schedule-picker"
import { NotificationChannelPicker } from "./notification-channel-picker"
import {
  useAutomationEditor,
  type AutomationEditorProps,
} from "./use-automation-editor"
const DEFAULT_FOLDER_VALUE = "__iyw_claw_default_folder__"

export function AutomationEditor(props: AutomationEditorProps) {
  const { automation, onCancel, onBackToTemplates } = props
  const {
    t,
    name,
    setName,
    agentType,
    setAgentType,
    setPrompt,
    folderId,
    setFolderId,
    cron,
    setCron,
    timezone,
    configValues,
    setConfigValues,
    notificationChannelId,
    setNotificationChannelId,
    error,
    saving,
    nextRun,
    editorRef,
    userChoseAgentRef,
    folderPathResolving,
    referenceGroupLabels,
    mentionUiLabels,
    referenceSearch,
    agentOptions,
    invocations,
    folders,
    submit,
  } = useAutomationEditor(props)
  return (
    <div className="grid min-w-0 grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1.4fr)_minmax(280px,1fr)]">
      {onBackToTemplates ? (
        <button
          type="button"
          onClick={onBackToTemplates}
          className="-ml-1 inline-flex w-fit lg:col-span-2 items-center gap-1 text-xs text-muted-foreground transition-colors hover:text-foreground"
        >
          <ArrowLeft className="h-3.5 w-3.5" aria-hidden="true" />
          {t("backToTemplates")}
        </button>
      ) : null}

      <section className="flex min-w-0 flex-col gap-4">
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

        {/* Target — use an explicit workspace or a task-specific managed folder. */}
        <div className="flex flex-col gap-2">
          <h3 className="text-[0.6875rem] font-medium uppercase tracking-wide text-muted-foreground">
            {t("sectionTarget")}
          </h3>
          <div className="flex flex-wrap items-center gap-2">
            <Select
              value={folderId != null ? String(folderId) : DEFAULT_FOLDER_VALUE}
              onValueChange={(v) =>
                setFolderId(v === DEFAULT_FOLDER_VALUE ? null : Number(v))
              }
            >
              <SelectTrigger size="sm" className="h-7 gap-1.5 text-xs">
                <Folder
                  className="size-3.5 text-muted-foreground"
                  aria-hidden="true"
                />
                <SelectValue placeholder={t("folderPlaceholder")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={DEFAULT_FOLDER_VALUE}>
                  {t("folderDefault")}
                </SelectItem>
                {folders.map((f) => (
                  <SelectItem key={f.id} value={String(f.id)}>
                    {f.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          {/* Explain whether this task receives a managed folder or shares an
            explicit workspace's working tree. */}
          <p className="text-xs text-muted-foreground">
            {folderId == null
              ? t("folderDefaultHint")
              : t("isolationSharedCaveat")}
          </p>
        </div>
      </section>
      <section className="flex min-w-0 flex-col gap-5">
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

        <div className="rounded-xl border border-border bg-card p-4">
          <NotificationChannelPicker
            value={notificationChannelId}
            onChange={setNotificationChannelId}
          />
        </div>
      </section>
      {error ? (
        <p className="text-sm text-destructive lg:col-span-2" role="alert">
          {error}
        </p>
      ) : null}

      <div className="mt-1 flex justify-end gap-2 border-t border-border pt-5 lg:col-span-2">
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
