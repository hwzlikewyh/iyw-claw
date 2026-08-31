import { mergeAttributes, Node, type JSONContent } from "@tiptap/core"
import { ReactNodeViewRenderer } from "@tiptap/react"

import { ImageAttachmentView } from "./image-attachment-view"

export const IMAGE_ATTACHMENT_NODE = "imageAttachment"

export type ImageAttachmentStatus = "uploading" | "ready" | "failed"

export interface ImageAttachmentAttrs {
  attachmentId: string
  name: string
  mimeType: string
  uri: string | null
  localPath: string | null
  status: ImageAttachmentStatus
  /** Runtime-only preview URL. The draft saver removes this before storage. */
  previewUrl?: string
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    imageAttachment: {
      insertImageAttachment: (attrs: ImageAttachmentAttrs) => ReturnType
      updateImageAttachment: (
        attachmentId: string,
        attrs: Partial<ImageAttachmentAttrs>
      ) => ReturnType
    }
  }
}

export const ImageAttachment = Node.create({
  name: IMAGE_ATTACHMENT_NODE,
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  draggable: false,

  addAttributes() {
    return {
      attachmentId: { default: "" },
      name: { default: "image" },
      mimeType: { default: "image/png" },
      uri: { default: null },
      localPath: { default: null },
      status: { default: "uploading" },
      previewUrl: { default: null },
    }
  },

  parseHTML() {
    return [{ tag: "span[data-image-attachment]" }]
  },

  renderHTML({ node, HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        "data-image-attachment": "",
        "data-attachment-id": node.attrs.attachmentId,
      }),
      node.attrs.name,
    ]
  },

  renderText({ node }) {
    return `[${String(node.attrs.name || "image")}]`
  },

  renderMarkdown(node: JSONContent) {
    return `[${String(node.attrs?.name || "image")}]`
  },

  addNodeView() {
    return ReactNodeViewRenderer(ImageAttachmentView)
  },

  addCommands() {
    return {
      insertImageAttachment:
        (attrs: ImageAttachmentAttrs) =>
        ({ commands }) =>
          commands.insertContent({ type: IMAGE_ATTACHMENT_NODE, attrs }),
      updateImageAttachment:
        (attachmentId: string, attrs: Partial<ImageAttachmentAttrs>) =>
        ({ tr, state, dispatch }) => {
          let updated = false
          state.doc.descendants((node, pos) => {
            if (
              node.type.name !== IMAGE_ATTACHMENT_NODE ||
              node.attrs.attachmentId !== attachmentId
            ) {
              return true
            }
            if (dispatch) {
              tr.setNodeMarkup(pos, undefined, { ...node.attrs, ...attrs })
              tr.setMeta("addToHistory", false)
            }
            updated = true
            return false
          })
          if (updated && dispatch) dispatch(tr)
          return updated
        },
    }
  },
})
