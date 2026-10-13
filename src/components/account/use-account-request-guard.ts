"use client"

import { useOptionalIywAccount } from "@/contexts/iyw-account-context"

export function useAccountRequestsBlocked() {
  const account = useOptionalIywAccount()
  return Boolean(
    account?.profile?.workspace_pending ||
    (account?.status === "authenticated" && account.actionLoading)
  )
}
