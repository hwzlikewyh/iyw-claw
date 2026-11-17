import type { UploadDetailsProps } from "./upload-form"
import { UploadPublishFields } from "./upload-publish-fields"
import { UploadVersionFields } from "./upload-version-fields"

export function UploadDetailsStep(props: UploadDetailsProps) {
  return (
    <fieldset disabled={props.busy} className="grid gap-3">
      {props.isPublish ? <UploadPublishFields {...props} /> : null}
      <UploadVersionFields {...props} />
    </fieldset>
  )
}
