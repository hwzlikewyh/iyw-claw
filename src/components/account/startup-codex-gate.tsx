"use client"

import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react"

import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import {
  acpDetectAgentLocalVersion,
  acpListAgents,
  acpPrepareNpxAgent,
} from "@/lib/api"
import { randomUUID } from "@/lib/utils"

type CodexBootstrapState =
  | "idle"
  | "checking"
  | "installing"
  | "ready"
  | "error"

export function StartupCodexGate({ children }: { children: ReactNode }) {
  const t = useTranslations("StartupCodex")
  const { status } = useIywAccount()
  const { refresh: refreshAgents } = useAcpAgents()
  const [state, setState] = useState<CodexBootstrapState>("idle")
  const runningRef = useRef(false)
  const taskIdRef = useRef(randomUUID())
  const workspaceRef = useRef<HTMLDivElement>(null)
  const blocked = status === "authenticated" && state !== "ready"

  const bootstrap = useCallback(async () => {
    if (runningRef.current) return
    runningRef.current = true
    setState("checking")
    try {
      const agents = await acpListAgents()
      const codex = agents.find((agent) => agent.agent_type === "codex")
      if (!codex) throw new Error("Codex is missing from the Agent registry")
      const installed = await acpDetectAgentLocalVersion("codex")
      if (installed) {
        await refreshAgents()
        setState("ready")
        return
      }
      setState("installing")
      await acpPrepareNpxAgent(
        "codex",
        codex.registry_version,
        taskIdRef.current,
        false
      )
      await refreshAgents()
      setState("ready")
    } catch {
      setState("error")
    } finally {
      runningRef.current = false
    }
  }, [refreshAgents])

  useEffect(() => {
    if (status === "authenticated" && state === "idle") void bootstrap()
  }, [bootstrap, state, status])

  useEffect(() => {
    const roots = workspaceRef.current?.children ?? []
    for (const root of roots) {
      if (blocked) root.setAttribute("inert", "")
      else root.removeAttribute("inert")
    }
  }, [blocked])

  const title =
    state === "installing" ? t("installingTitle") : t("checkingTitle")
  const description =
    state === "installing"
      ? t("installingDescription")
      : t("checkingDescription")

  return (
    <>
      <div ref={workspaceRef} inert={blocked ? true : undefined}>
        {children}
      </div>
      <Dialog open={blocked} onOpenChange={() => {}}>
        <DialogContent
          className="max-w-md rounded-lg"
          showCloseButton={false}
          onEscapeKeyDown={(event) => event.preventDefault()}
          onPointerDownOutside={(event) => event.preventDefault()}
          onInteractOutside={(event) => event.preventDefault()}
        >
          <DialogHeader className="text-center">
            {state !== "error" ? (
              <Loader2 className="mx-auto mb-2 h-7 w-7 animate-spin text-muted-foreground" />
            ) : null}
            <DialogTitle>
              {state === "error" ? t("errorTitle") : title}
            </DialogTitle>
            <DialogDescription>
              {state === "error" ? t("errorDescription") : description}
            </DialogDescription>
          </DialogHeader>
          {state !== "error" ? (
            <Progress
              value={state === "installing" ? 75 : 30}
              aria-label={title}
              className="h-2"
            />
          ) : null}
          {state === "error" ? (
            <div className="grid gap-4 text-center">
              <Button
                size="sm"
                className="mx-auto"
                onClick={() => void bootstrap()}
              >
                {t("retry")}
              </Button>
            </div>
          ) : null}
        </DialogContent>
      </Dialog>
    </>
  )
}
