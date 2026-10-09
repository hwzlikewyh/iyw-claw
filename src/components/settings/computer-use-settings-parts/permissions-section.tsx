"use client"

import { useIsMac } from "@/hooks/use-is-mac"
import { computerServerPlatform } from "@/lib/computer/computer-api"
import { MacPermissions } from "./mac-permissions"

export function PermissionsSection({ enabled }: { enabled: boolean }) {
  const isMac = useIsMac()
  const server = computerServerPlatform()
  if (!(server ? server === "macos" : isMac)) return null
  return <MacPermissions enabled={enabled} />
}
