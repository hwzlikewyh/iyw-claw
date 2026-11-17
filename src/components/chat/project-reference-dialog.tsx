"use client"

import { useState } from "react"
import {
  AlertCircle,
  ChevronRight,
  Folder,
  FolderOpen,
  PackageOpen,
} from "lucide-react"
import { useTranslations } from "next-intl"

import { DirectoryBrowserDialog } from "@/components/shared/directory-browser-dialog"
import { Button } from "@/components/ui/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { isLocalDesktop } from "@/lib/platform"
import {
  ArtifactReferencePicker,
  useReferenceArtifacts,
  type ReferenceArtifactsState,
  type ProjectReferenceSelection,
} from "./project-reference-artifacts"
import { ProjectReferenceFooter } from "./project-reference-footer"
import { WorkspaceReferencePicker } from "./project-reference-workspace"

export type { ProjectReferenceSelection } from "./project-reference-artifacts"

interface ProjectReferenceDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  rootPath: string | null
  onSelect: (selection: ProjectReferenceSelection) => void
  onBrowseFolder: () => void | Promise<void>
}

interface ReferencePickerProps {
  selected: ProjectReferenceSelection | null
  onSelect: (selection: ProjectReferenceSelection) => void
}

function pathName(path: string): string {
  return (
    path
      .replace(/[\\/]+$/, "")
      .split(/[\\/]/)
      .pop() || path
  )
}

function WorkspaceRootRow({
  rootPath,
  onSelect,
}: {
  rootPath: string
  onSelect: ReferencePickerProps["onSelect"]
}) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  return (
    <button
      type="button"
      onClick={() =>
        onSelect({ path: rootPath, name: pathName(rootPath), kind: "dir" })
      }
      className="flex w-full items-center gap-3 rounded-md bg-muted/40 px-3 py-3 text-left hover:bg-muted focus-visible:outline focus-visible:outline-ring"
    >
      <Folder className="size-4 shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1">
        <span className="block text-sm font-medium">{t("wholeWorkspace")}</span>
        <span
          className="block truncate text-xs text-muted-foreground"
          title={rootPath}
        >
          {rootPath}
        </span>
      </span>
      <ChevronRight className="size-4 shrink-0 text-muted-foreground" />
    </button>
  )
}

function ReferenceState({
  icon: Icon,
  text,
}: {
  icon: typeof AlertCircle
  text: string
}) {
  return (
    <div className="flex min-h-48 items-center justify-center gap-2 text-sm text-muted-foreground">
      <Icon className="size-4" />
      {text}
    </div>
  )
}

export function ProjectReferenceDialog({
  open,
  onOpenChange,
  rootPath,
  onSelect,
  onBrowseFolder,
}: ProjectReferenceDialogProps) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  const { activeFolderId } = useActiveFolder()
  const artifacts = useReferenceArtifacts(open, activeFolderId)
  const [selected, setSelected] = useState<ProjectReferenceSelection | null>(
    null
  )
  const [browsing, setBrowsing] = useState(false)

  const changeOpen = (next: boolean) => {
    if (!next) setSelected(null)
    onOpenChange(next)
  }
  const confirm = () => {
    if (!selected) return
    onSelect(selected)
    changeOpen(false)
  }
  const browseFolder = () => {
    if (!isLocalDesktop()) {
      setBrowsing(true)
      return
    }
    changeOpen(false)
    void onBrowseFolder()
  }

  return (
    <>
      <Dialog open={open && !browsing} onOpenChange={changeOpen}>
        <DialogContent className="max-w-xl gap-4 rounded-lg p-5">
          <DialogHeader>
            <DialogTitle>{t("title")}</DialogTitle>
            <DialogDescription>{t("description")}</DialogDescription>
          </DialogHeader>
          <Button
            variant={selected ? "outline" : "default"}
            onClick={browseFolder}
            className="w-full justify-start"
          >
            <FolderOpen className="size-4" />
            {t("browseFolder")}
          </Button>
          <ReferenceTabs
            key={rootPath}
            rootPath={rootPath}
            artifacts={artifacts}
            selected={selected}
            onSelect={setSelected}
            onUseWorkspace={(selection) => {
              onSelect(selection)
              changeOpen(false)
            }}
          />
          <ProjectReferenceFooter
            selected={selected}
            onCancel={() => changeOpen(false)}
            onConfirm={confirm}
          />
        </DialogContent>
      </Dialog>
      <DirectoryBrowserDialog
        open={open && browsing}
        onOpenChange={setBrowsing}
        title={t("browseFolder")}
        initialPath={rootPath ?? undefined}
        onSelect={(path) => {
          onSelect({ path, name: pathName(path), kind: "dir" })
          setBrowsing(false)
          changeOpen(false)
        }}
      />
    </>
  )
}

function ReferenceTabs({
  rootPath,
  artifacts,
  selected,
  onSelect,
  onUseWorkspace,
}: ReferencePickerProps & {
  rootPath: string | null
  artifacts: ReferenceArtifactsState
  onUseWorkspace: ReferencePickerProps["onSelect"]
}) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  return (
    <Tabs
      defaultValue={rootPath ? "workspace" : "artifacts"}
      className="min-h-0"
    >
      <TabsList className="h-9 w-full rounded-md">
        <TabsTrigger
          value="workspace"
          disabled={!rootPath}
          className="rounded-sm"
        >
          <Folder className="size-4" />
          {t("workspace")}
        </TabsTrigger>
        <TabsTrigger value="artifacts" className="rounded-sm">
          <PackageOpen className="size-4" />
          {t("artifacts")}
        </TabsTrigger>
      </TabsList>
      <TabsContent value="workspace" className="min-h-0">
        {rootPath ? (
          <div className="space-y-2">
            <WorkspaceRootRow rootPath={rootPath} onSelect={onUseWorkspace} />
            <Collapsible>
              <CollapsibleTrigger className="group flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm hover:bg-muted/50 focus-visible:outline focus-visible:outline-ring">
                <ChevronRight className="size-4 group-data-[state=open]:rotate-90" />
                {t("browseWorkspace")}
              </CollapsibleTrigger>
              <CollapsibleContent>
                <WorkspaceReferencePicker
                  rootPath={rootPath}
                  selected={selected}
                  onSelect={onSelect}
                />
              </CollapsibleContent>
            </Collapsible>
          </div>
        ) : (
          <ReferenceState icon={AlertCircle} text={t("noWorkspace")} />
        )}
      </TabsContent>
      <TabsContent
        value="artifacts"
        className="h-64 min-h-0 overflow-hidden rounded-md border"
      >
        <ArtifactReferencePicker
          {...artifacts}
          selected={selected}
          onSelect={onSelect}
        />
      </TabsContent>
    </Tabs>
  )
}
