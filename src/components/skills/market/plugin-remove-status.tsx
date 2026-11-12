"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { getTransport, onTransportChange } from "@/lib/transport"
import { readPluginRemoval } from "@/lib/plugin-remove"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { PluginRemoveDialog } from "./plugin-remove-dialog"

export function PluginRemoveStatus({
  id,
  onChanged,
}: {
  id: string
  onChanged: () => void
}) {
  const { profile, status } = useIywAccount()
  const [scope, setScope] = useState(0)
  useEffect(() => onTransportChange(() => setScope((value) => value + 1)), [])
  return (
    <StatusContent
      key={JSON.stringify([
        id,
        profile?.user_id,
        profile?.org_code,
        status,
        scope,
      ])}
      id={id}
      onChanged={onChanged}
    />
  )
}

function StatusContent({
  id,
  onChanged,
}: {
  id: string
  onChanged: () => void
}) {
  const t = useTranslations("PluginRemoval")
  const [revision, setRevision] = useState(0),
    [pending, setPending] = useState(false),
    [open, setOpen] = useState(false)
  useStatusRead(id, revision, setPending)
  return (
    <>
      {pending ? (
        <Button
          size="sm"
          variant="outline"
          className="mt-3 w-full"
          onClick={() => setOpen(true)}
        >
          {t("pending")}
        </Button>
      ) : null}
      {open ? (
        <PluginRemoveDialog
          id={id}
          history
          onClose={() => {
            setOpen(false)
            setRevision((value) => value + 1)
          }}
          onRemoved={onChanged}
          onRecovered={onChanged}
        />
      ) : null}
    </>
  )
}

function useStatusRead(
  id: string,
  revision: number,
  setPending: (value: boolean) => void
) {
  useEffect(() => {
    let alive = true
    void readPluginRemoval(id, getTransport())
      .then((value) => {
        if (alive)
          setPending(
            value?.state === "cleanup_pending" ||
              value?.state === "recovery_required"
          )
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [id, revision, setPending])
}
