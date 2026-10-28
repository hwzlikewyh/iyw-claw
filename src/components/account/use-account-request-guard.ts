"use client"

import { useOptionalIywAccount } from "@/contexts/iyw-account-context"

export function hasAvailableAccountPoints(
  balancePoints: number | null | undefined
) {
  return (
    typeof balancePoints === "number" &&
    Number.isFinite(balancePoints) &&
    balancePoints > 0
  )
}

export function useAccountPointsInsufficient() {
  const account = useOptionalIywAccount()
  return Boolean(
    account && !hasAvailableAccountPoints(account.profile?.balance_points)
  )
}

export function useAccountRequestsBlocked() {
  const account = useOptionalIywAccount()
  return Boolean(
    account?.profile?.workspace_pending ||
    (account?.status === "authenticated" && account.actionLoading)
  )
}

export function useAccountChatRequestsBlocked() {
  const requestsBlocked = useAccountRequestsBlocked()
  const pointsInsufficient = useAccountPointsInsufficient()
  return requestsBlocked || pointsInsufficient
}
