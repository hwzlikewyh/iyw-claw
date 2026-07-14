"use client"

import { Loader2, LogOut } from "lucide-react"
import { useTranslations } from "next-intl"

import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import type { IywAccountProfile } from "@/lib/types"

const DEFAULT_AVATAR_URL =
  "https://chdesign.oss-cn-shanghai.aliyuncs.com/static/avatar/default.png?x-oss-process=image/resize,m_mfit,h_35"

export function displayName(
  profile: IywAccountProfile | null,
  fallback: string
) {
  if (!profile?.logged_in) return fallback
  return (
    profile.nick_name?.trim() ||
    profile.name?.trim() ||
    profile.phone?.trim() ||
    fallback
  )
}

export function balancePoints(
  profile: IywAccountProfile | null,
  fallback: string
) {
  return profile?.balance_points === null ||
    profile?.balance_points === undefined
    ? fallback
    : String(profile.balance_points)
}

export function normalizeAvatarUrl(value: string | null | undefined) {
  const trimmed = value?.trim()
  if (!trimmed) return null
  const normalizedPath = (trimmed.split("?")[0] ?? "")
    .replace(/^https?:\/\/account\.iyw\.cn\//i, "")
    .replace(/^\/+/, "")
  return normalizedPath === "static/avatar/default.png"
    ? DEFAULT_AVATAR_URL
    : trimmed
}

export function AccountAvatar({
  profile,
  className,
}: {
  profile: IywAccountProfile | null
  className?: string
}) {
  const avatarUrl =
    normalizeAvatarUrl(profile?.avatar_url) ||
    (profile?.logged_in ? DEFAULT_AVATAR_URL : "")
  const fallback = displayName(profile, "").slice(0, 1).toUpperCase() || "I"
  return (
    <Avatar className={className}>
      {avatarUrl ? (
        <AvatarImage
          src={avatarUrl}
          alt={displayName(profile, "iyw")}
          referrerPolicy="no-referrer"
        />
      ) : null}
      <AvatarFallback>{fallback}</AvatarFallback>
    </Avatar>
  )
}

export function AccountProfilePanel({
  profile,
  loading,
  onLogout,
}: {
  profile: IywAccountProfile
  loading: boolean
  onLogout: () => void
}) {
  const t = useTranslations("SidebarAccount")
  return (
    <div className="flex min-h-[20rem] flex-col gap-4 p-5">
      <div className="flex items-center gap-3">
        <AccountAvatar profile={profile} className="h-11 w-11" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-base font-medium">
            {displayName(profile, t("signedIn"))}
          </div>
          <div className="mt-0.5 truncate text-sm text-muted-foreground">
            {profile.org_name?.trim() ||
              profile.phone?.trim() ||
              t("accountReady")}
          </div>
        </div>
      </div>
      <div className="grid gap-2 rounded-lg border bg-muted/25 p-3">
        <InfoRow
          label={t("balancePoints")}
          value={t("balanceValue", {
            points: balancePoints(profile, t("balanceUnknown")),
          })}
          mono
        />
        {profile.balance_expiry_time ? (
          <InfoRow
            label={t("balanceExpiry")}
            value={profile.balance_expiry_time}
          />
        ) : null}
      </div>
      <div className="grid gap-2 rounded-lg border bg-muted/25 p-3">
        <InfoRow label={t("userId")} value={profile.user_id} />
        <InfoRow label={t("phone")} value={profile.phone} />
        <InfoRow label={t("organization")} value={profile.org_name} />
      </div>
      <div className="mt-auto flex justify-end">
        <Button
          variant="outline"
          size="sm"
          onClick={onLogout}
          disabled={loading}
        >
          {loading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <LogOut className="h-3.5 w-3.5" />
          )}
          {t("logout")}
        </Button>
      </div>
    </div>
  )
}

function InfoRow({
  label,
  value,
  mono = false,
}: {
  label: string
  value: string | null | undefined
  mono?: boolean
}) {
  const t = useTranslations("SidebarAccount")
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-sm text-muted-foreground">{label}</span>
      <span
        className={mono ? "font-mono text-sm font-medium" : "truncate text-sm"}
      >
        {value?.trim() || t("emptyValue")}
      </span>
    </div>
  )
}
