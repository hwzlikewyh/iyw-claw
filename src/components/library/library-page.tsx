"use client"

import { useState, useSyncExternalStore } from "react"
import { BookOpenText } from "lucide-react"
import { useTranslations } from "next-intl"
import { useOptionalIywAccount } from "@/contexts/iyw-account-context"
import { useAccountRequestsBlocked } from "@/components/account/use-account-request-guard"
import {
  getTransport,
  onTransportChange,
  type Transport,
} from "@/lib/transport"
import type { KnowledgeCategory } from "@/lib/cloud-knowledge"
import { cn } from "@/lib/utils"
import { LibraryBrowser } from "./library-browser"
import { LibraryLoading } from "./library-states"

const transportIds = new WeakMap<Transport, number>()
let nextTransportId = 0
function transportIdentity(transport: Transport) {
  if (!transportIds.has(transport))
    transportIds.set(transport, ++nextTransportId)
  return transportIds.get(transport)
}
const serverTransport = () => null

export function LibraryPage() {
  const t = useTranslations("Library")
  const account = useOptionalIywAccount()
  const blocked = useAccountRequestsBlocked()
  const transport = useSyncExternalStore(
    onTransportChange,
    getTransport,
    serverTransport
  )
  const [category, setCategory] = useState<KnowledgeCategory>(0)
  const identity = [
    account?.status,
    account?.profile?.user_id,
    account?.profile?.org_code,
    transport && transportIdentity(transport),
  ].join(":")
  const unavailable =
    account?.status === "login_required" || account?.status === "error"
  return (
    <section
      className="flex h-full min-h-0 min-w-0 flex-col bg-background"
      aria-label={t("title")}
    >
      <LibraryHeader category={category} onCategory={setCategory} />
      {blocked || !transport || account?.status === "checking" ? (
        <LibraryLoading />
      ) : unavailable ? (
        <p className="p-6 text-sm text-muted-foreground">
          {t("loginRequired")}
        </p>
      ) : (
        <LibraryBrowser
          key={identity + category}
          transport={transport}
          category={category}
        />
      )}
    </section>
  )
}

function LibraryHeader({
  category,
  onCategory,
}: {
  category: KnowledgeCategory
  onCategory: (category: KnowledgeCategory) => void
}) {
  const t = useTranslations("Library")
  return (
    <header className="shrink-0 border-b px-4 pt-5 sm:px-6">
      <div className="flex items-center gap-2.5">
        <BookOpenText
          className="size-5 text-muted-foreground"
          aria-hidden="true"
        />
        <h1 className="text-xl font-semibold">{t("title")}</h1>
      </div>
      <nav aria-label={t("categories")} className="mt-4 flex gap-6">
        {([0, 1] as const).map((value) => (
          <button
            key={value}
            type="button"
            aria-pressed={category === value}
            onClick={() => onCategory(value)}
            className={cn(
              "min-h-10 border-b-2 px-1 pb-2 text-sm focus-visible:outline focus-visible:outline-ring",
              category === value
                ? "border-foreground font-medium"
                : "border-transparent text-muted-foreground hover:text-foreground"
            )}
          >
            {t(value === 0 ? "knowledge" : "customers")}
          </button>
        ))}
      </nav>
    </header>
  )
}
