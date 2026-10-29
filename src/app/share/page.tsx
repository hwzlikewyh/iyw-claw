import type { Metadata } from "next"
import { SharedConversationPage } from "@/components/share/shared-conversation-page"

export const metadata: Metadata = {
  title: "会话分享 · 原助理",
  robots: { index: false, follow: false },
  referrer: "no-referrer",
}

export default function SharePage() {
  return <SharedConversationPage />
}
