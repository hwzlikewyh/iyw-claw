"use client"

import { useOptionalIywAccount } from "@/contexts/iyw-account-context"

export function hasKnownAccountPoints(
  balancePoints: number | null | undefined
): balancePoints is number {
  return (
    typeof balancePoints === "number" &&
    Number.isFinite(balancePoints) &&
    balancePoints >= 0
  )
}

export function hasAvailableAccountPoints(
  balancePoints: number | null | undefined
) {
  return hasKnownAccountPoints(balancePoints) && balancePoints > 0
}

export function useAccountPointsInsufficient() {
  const account = useOptionalIywAccount()
  const balance = account?.profile?.balance_points
  return Boolean(account && hasKnownAccountPoints(balance) && balance === 0)
}

export function useAccountRequestsBlocked() {
  const account = useOptionalIywAccount()
  return Boolean(
    account?.profile?.workspace_pending ||
    (account?.status === "authenticated" && account.actionLoading)
  )
}

export function useAccountChatRequestsBlocked() {
  // 点数缓存只用于展示，消费准入由后端检查；空间切换仍阻止新请求。
  return useAccountRequestsBlocked()
}
