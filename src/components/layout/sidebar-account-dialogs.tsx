"use client"

import { useTranslations } from "next-intl"
import { AccountLoginPanel } from "@/components/account/account-login-panel"
import { AccountProfilePanel } from "@/components/account/account-profile-panel"
import { ConversationManageDialog } from "@/components/conversations/conversation-manage-dialog"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { useSidebarAccount } from "./use-sidebar-account"

type Account = ReturnType<typeof useSidebarAccount>

export function SidebarAccountDialogs({ account }: { account: Account }) {
  const t = useTranslations("SidebarAccount")
  const detailsOpen = account.dialog === "profile" || account.dialog === "login"
  return (
    <>
      <Dialog
        open={detailsOpen}
        onOpenChange={(open) => {
          if (!open) account.showDialog(null)
        }}
      >
        <DialogContent className="max-h-[calc(100dvh-2rem)] max-w-lg overflow-y-auto rounded-lg p-0">
          <DialogHeader className="px-5 pt-5">
            <DialogTitle>{t("dialogTitle")}</DialogTitle>
            <DialogDescription>
              {account.profile?.logged_in
                ? t("signedInDescription")
                : t("dialogHint")}
            </DialogDescription>
          </DialogHeader>
          {account.profile?.logged_in ? (
            <AccountProfilePanel
              profile={account.profile}
              loading={account.actionLoading}
              refreshing={account.refreshing}
              onRefresh={() => void account.refresh()}
              onLogout={() => account.showDialog("logout")}
            />
          ) : (
            <AccountLoginPanel
              active={detailsOpen}
              onAuthenticated={() => account.showDialog(null)}
            />
          )}
        </DialogContent>
      </Dialog>
      <AccountLogout account={account} />
      {account.dialog === "manager" && (
        <ConversationManageDialog
          open
          onOpenChange={(open) => {
            if (!open) account.showDialog(null)
          }}
        />
      )}
    </>
  )
}

function AccountLogout({ account }: { account: Account }) {
  const t = useTranslations("SidebarDesign")
  const common = useTranslations("Folder.common")
  const labels = useTranslations("SidebarAccount")
  return (
    <AlertDialog
      open={account.dialog === "logout"}
      onOpenChange={(open) => {
        if (!open && !account.actionLoading) account.showDialog(null)
      }}
    >
      <AlertDialogContent className="max-w-sm rounded-lg">
        <AlertDialogHeader>
          <AlertDialogTitle>{t("logoutTitle")}</AlertDialogTitle>
          <AlertDialogDescription>
            {t("logoutDescription")}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={account.actionLoading}>
            {common("cancel")}
          </AlertDialogCancel>
          <AlertDialogAction
            disabled={account.actionLoading}
            className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            onClick={(event) => {
              event.preventDefault()
              void account.signOut()
            }}
          >
            {labels("logout")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
