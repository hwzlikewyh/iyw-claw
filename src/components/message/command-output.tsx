"use client"

import { useRef, useState } from "react"
import { ArrowDown, ChevronDown, Copy, Check } from "lucide-react"
import { useTranslations } from "next-intl"
import { ToolOutput } from "@/components/ai-elements/tool"
import { Terminal, TerminalContent } from "@/components/ai-elements/terminal"
import { copyTextToClipboard } from "@/lib/utils"
import { useProcessAutoFollow } from "./use-process-auto-follow"
import { useContentReading } from "./use-content-reading"

interface CommandOutputProps {
  output?: string | null
  errorText?: string | null
  exitCode: number | null
  running: boolean
}

function OutputCopyButton({ output }: { output: string }) {
  const t = useTranslations("Folder.chat.messageList")
  const [copiedOutput, setCopiedOutput] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)
  const copied = copiedOutput === output
  const copy = async () => {
    const ok = await copyTextToClipboard(output)
    setFailed(!ok)
    if (ok) setCopiedOutput(output)
  }
  return (
    <>
      <button
        type="button"
        className="assistant-text-action"
        aria-label={t(copied ? "copied" : "copyOutput")}
        onClick={copy}
      >
        {copied ? (
          <Check className="size-3.5" />
        ) : (
          <Copy className="size-3.5" />
        )}
      </button>
      {failed && (
        <span role="status" className="text-xs text-destructive">
          {t("copyOutputFailed")}
        </span>
      )}
    </>
  )
}

function OutputHeader({
  output,
  exitCode,
  expanded,
  onToggle,
}: Pick<CommandOutputProps, "output" | "exitCode"> & {
  expanded: boolean
  onToggle: () => void
}) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
      <span>
        {t("commandOutput")}
        {exitCode !== null && (
          <span className={exitCode !== 0 ? "ms-2 text-destructive" : "ms-2"}>
            {t("commandExitCode", { code: exitCode })}
          </span>
        )}
      </span>
      <div className="flex items-center gap-3">
        {output && <OutputCopyButton output={output} />}
        <button
          type="button"
          className="assistant-text-action inline-flex items-center gap-1"
          aria-expanded={expanded}
          onClick={onToggle}
        >
          <ChevronDown className={`size-3 ${expanded ? "rotate-180" : ""}`} />
          {t(expanded ? "collapseOutput" : "expandOutput")}
        </button>
      </div>
    </div>
  )
}

function CommandTerminal({
  output,
  running,
}: Pick<CommandOutputProps, "output" | "running">) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <Terminal
      output={output || t(running ? "commandWaitingOutput" : "commandNoOutput")}
      isStreaming={running}
      autoScroll={false}
      className="rounded-none border-0 bg-transparent"
    >
      <TerminalContent className="max-h-none p-3 text-xs leading-5 [&_pre]:whitespace-pre [&_pre]:break-normal" />
    </Terminal>
  )
}

export function CommandOutput({
  output,
  errorText,
  exitCode,
  running,
}: CommandOutputProps) {
  const t = useTranslations("Folder.chat.messageList")
  const [expanded, setExpanded] = useState(false)
  const readingRef = useRef<HTMLDivElement>(null)
  const reading = useContentReading(readingRef)
  const { viewportRef, handleScroll, isFollowing, scrollToLatest } =
    useProcessAutoFollow(output, running && !reading)
  return (
    <div
      ref={readingRef}
      className="assistant-command-output min-w-0 space-y-2"
    >
      <OutputHeader
        output={output}
        exitCode={exitCode}
        expanded={expanded}
        onToggle={() => setExpanded(!expanded)}
      />
      <div
        ref={viewportRef}
        onScroll={handleScroll}
        className={
          expanded
            ? "overflow-x-auto rounded-md border bg-muted/30"
            : "max-h-56 overflow-auto overscroll-contain rounded-md border bg-muted/30"
        }
      >
        <CommandTerminal output={output} running={running} />
      </div>
      {!isFollowing && running && (
        <button
          type="button"
          className="assistant-text-action inline-flex items-center gap-1 text-xs"
          onClick={scrollToLatest}
        >
          <ArrowDown className="size-3" />
          {t("backToLatest")}
        </button>
      )}
      {errorText && <ToolOutput output={null} errorText={errorText} />}
    </div>
  )
}
