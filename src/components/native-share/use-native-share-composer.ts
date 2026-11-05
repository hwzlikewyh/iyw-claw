"use client"

import { useEffect, type RefObject } from "react"
import type { Editor, JSONContent } from "@tiptap/core"
import { useTranslations } from "next-intl"
import type { RichComposerHandle } from "@/components/chat/composer/rich-composer"
import { NATIVE_SHARE_DRAFT, type ShareDraftRequest } from "@/lib/native-share"
import { persistMessageInputDraftV2Now } from "@/lib/message-input-draft"
import { buildFileUri } from "@/lib/reference-link"
import { isRemoteDesktopMode } from "@/lib/transport"

interface ComposerOptions {
  tabId?: string | null
  draftKey: string | null
  enabled: boolean
  editorRef: RefObject<RichComposerHandle | null>
}

function importedNodes(request: ShareDraftRequest): JSONContent[] {
  const content: JSONContent[] = [{ type: "text", text: request.prompt }]
  request.paths.forEach((path, index) => {
    content.push(
      { type: "text", text: " " },
      {
        type: "reference",
        attrs: {
          refType: "file",
          id: `native-share:${request.batchId}:${index}`,
          label: path.split(/[/\\]/).pop() || path,
          uri: buildFileUri(path),
          meta: { fileKind: "file" },
        },
      }
    )
  })
  return [{ type: "paragraph", content }]
}

function hasBatch(editor: Editor, id: string): boolean {
  let present = false
  editor.state.doc.descendants((node) => {
    if (
      node.type.name === "reference" &&
      String(node.attrs.id).startsWith(`native-share:${id}:`)
    )
      present = true
  })
  return present
}

export function useNativeShareComposer(options: ComposerOptions): void {
  const t = useTranslations("NativeShare")
  const { tabId, draftKey, enabled, editorRef } = options
  useEffect(() => {
    if (!tabId) return
    const receive = (event: Event) => {
      const request = (event as CustomEvent<ShareDraftRequest>).detail
      if (event.defaultPrevented || request?.tabId !== tabId) return
      event.preventDefault()
      const editor = editorRef.current?.getEditor()
      if (
        !enabled ||
        !draftKey ||
        !editor ||
        !editor.isEditable ||
        isRemoteDesktopMode()
      ) {
        request.finish(new Error(t("composerUnavailable")))
        return
      }
      let previous: JSONContent | null = null
      try {
        previous = editor.getJSON()
        if (
          !hasBatch(editor, request.batchId) &&
          !editor
            .chain()
            .focus("end")
            .insertContent(importedNodes(request))
            .run()
        ) {
          throw new Error(t("composerUnavailable"))
        }
        persistMessageInputDraftV2Now(draftKey, editor.getJSON())
        request.finish()
      } catch {
        try {
          if (previous) editorRef.current?.setDoc(previous)
        } finally {
          request.finish(new Error(t("draftSaveFailed")))
        }
      }
    }
    window.addEventListener(NATIVE_SHARE_DRAFT, receive)
    return () => window.removeEventListener(NATIVE_SHARE_DRAFT, receive)
  }, [tabId, draftKey, enabled, editorRef, t])
}
