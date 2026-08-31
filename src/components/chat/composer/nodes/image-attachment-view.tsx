import { LoaderCircle, RotateCcw, X, FileImage } from "lucide-react"
import { NodeViewWrapper, type ReactNodeViewProps } from "@tiptap/react"
import type { MouseEvent, SyntheticEvent } from "react"
import Image from "next/image"

import type { ImageAttachmentAttrs } from "./image-attachment-node"

export const IMAGE_ATTACHMENT_PREVIEW_EVENT =
  "iyw-claw:image-attachment-preview"
export const IMAGE_ATTACHMENT_RETRY_EVENT = "iyw-claw:image-attachment-retry"

interface ImageAttachmentEventDetail {
  attachmentId: string
}

function emitImageEvent(
  type: string,
  attachmentId: string,
  event: SyntheticEvent
): void {
  event.stopPropagation()
  window.dispatchEvent(
    new CustomEvent<ImageAttachmentEventDetail>(type, {
      detail: { attachmentId },
    })
  )
}

export function ImageAttachmentView({ node, deleteNode }: ReactNodeViewProps) {
  const attrs = node.attrs as ImageAttachmentAttrs
  const source = attrs.previewUrl || attrs.uri || ""
  const failed = attrs.status === "failed"
  const uploading = attrs.status === "uploading"

  return (
    <NodeViewWrapper
      as="span"
      className="iyw-claw-inline-image"
      contentEditable={false}
      data-image-attachment-id={attrs.attachmentId}
      data-image-attachment-status={attrs.status}
      onMouseDown={(event: MouseEvent<HTMLElement>) => event.stopPropagation()}
    >
      <span className="iyw-claw-inline-image-frame">
        {source ? (
          <button
            type="button"
            className="iyw-claw-inline-image-preview"
            onClick={(event: MouseEvent<HTMLButtonElement>) =>
              emitImageEvent(
                IMAGE_ATTACHMENT_PREVIEW_EVENT,
                attrs.attachmentId,
                event
              )
            }
            aria-label={`预览图片 ${attrs.name}`}
            title={attrs.name}
          >
            <Image
              src={source}
              alt={attrs.name}
              width={64}
              height={64}
              unoptimized
              draggable={false}
            />
          </button>
        ) : (
          <span className="iyw-claw-inline-image-placeholder" aria-hidden>
            <FileImage className="size-5" />
          </span>
        )}
        {uploading ? (
          <span
            className="iyw-claw-inline-image-status"
            role="status"
            aria-label={`正在上传 ${attrs.name}`}
          >
            <LoaderCircle className="size-3.5 animate-spin" />
          </span>
        ) : failed ? (
          <button
            type="button"
            className="iyw-claw-inline-image-status iyw-claw-inline-image-retry"
            onClick={(event) =>
              emitImageEvent(
                IMAGE_ATTACHMENT_RETRY_EVENT,
                attrs.attachmentId,
                event
              )
            }
            aria-label={`重试上传 ${attrs.name}`}
            title="重试上传"
          >
            <RotateCcw className="size-3.5" />
          </button>
        ) : null}
        <button
          type="button"
          className="iyw-claw-inline-image-remove"
          onClick={(event) => {
            event.stopPropagation()
            deleteNode()
          }}
          aria-label={`移除图片 ${attrs.name}`}
          title="移除图片"
        >
          <X className="size-3.5" />
        </button>
      </span>
      <span className="iyw-claw-inline-image-name" title={attrs.name}>
        {attrs.name}
      </span>
    </NodeViewWrapper>
  )
}
