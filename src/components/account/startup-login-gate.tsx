"use client"

import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { useEffect, useRef, type ReactNode } from "react"

import { AccountLoginPanel } from "@/components/account/account-login-panel"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useIywAccount } from "@/contexts/iyw-account-context"

export function StartupLoginGate({ children }: { children: ReactNode }) {
  const t = useTranslations("StartupLogin")
  const { status, error, refreshProfile } = useIywAccount()
  const blocked = status !== "authenticated"
  const workspaceRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const children = workspaceRef.current?.children ?? []
    for (const child of children) {
      if (blocked) child.setAttribute("inert", "")
      else child.removeAttribute("inert")
    }
  }, [blocked])

  return (
    <>
      <div ref={workspaceRef} inert={blocked ? true : undefined}>
        {children}
      </div>
      <Dialog open={blocked} onOpenChange={() => {}}>
        <DialogContent
          className="max-w-lg overflow-hidden rounded-lg p-0"
          showCloseButton={false}
          onEscapeKeyDown={(event) => event.preventDefault()}
          onPointerDownOutside={(event) => event.preventDefault()}
          onInteractOutside={(event) => event.preventDefault()}
        >
          {status === "checking" ? (
            <GateStatus
              title={t("checkingTitle")}
              description={t("checkingDescription")}
              loading
            />
          ) : status === "error" ? (
            <GateStatus
              title={t("errorTitle")}
              description={t("errorDescription")}
              detail={error}
              action={
                <Button size="sm" onClick={() => void refreshProfile()}>
                  {t("retry")}
                </Button>
              }
            />
          ) : (
            <>
              <DialogHeader className="px-5 pt-5">
                <DialogTitle>{t("title")}</DialogTitle>
                <DialogDescription>{t("description")}</DialogDescription>
              </DialogHeader>
              <div className="min-h-[24rem] border-t">
                <AccountLoginPanel />
              </div>
            </>
          )}
        </DialogContent>
      </Dialog>
    </>
  )
}

function GateStatus({
  title,
  description,
  detail,
  loading = false,
  action,
}: {
  title: string
  description: string
  detail?: string | null
  loading?: boolean
  action?: ReactNode
}) {
  return (
    <div className="grid min-h-64 place-items-center p-6 text-center">
      <DialogHeader className="max-w-sm">
        {loading ? (
          <Loader2 className="mx-auto mb-3 h-7 w-7 animate-spin text-muted-foreground" />
        ) : null}
        <DialogTitle>{title}</DialogTitle>
        <DialogDescription>{description}</DialogDescription>
        {detail ? (
          <p className="pt-2 text-xs text-destructive">{detail}</p>
        ) : null}
        {action ? (
          <div className="flex justify-center pt-3">{action}</div>
        ) : null}
      </DialogHeader>
    </div>
  )
}
