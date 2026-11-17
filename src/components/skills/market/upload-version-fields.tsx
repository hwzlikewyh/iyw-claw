import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { cn } from "@/lib/utils"
import type { UploadDetailsProps } from "./upload-form"
import { UploadFieldError } from "./upload-field-error"

function VersionField(props: UploadDetailsProps) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor="market-upload-version">{props.t("upload.version")}</Label>
      <Input
        id="market-upload-version"
        aria-invalid={Boolean(props.errors.version)}
        className={cn(
          "font-mono",
          props.errors.version && "border-destructive"
        )}
        value={props.form.version}
        onChange={(event) => props.update("version", event.target.value)}
        placeholder="1.0.0"
      />
      <UploadFieldError t={props.t} error={props.errors.version} />
    </div>
  )
}

function ChangelogField(props: UploadDetailsProps) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor="market-upload-changelog">
        {props.t("upload.changelog")}
      </Label>
      <Textarea
        id="market-upload-changelog"
        value={props.form.changelog}
        onChange={(event) => props.update("changelog", event.target.value)}
        className="min-h-16"
      />
    </div>
  )
}

function DependenciesField(props: UploadDetailsProps) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor="market-upload-deps">
        {props.t("upload.dependencies")}
      </Label>
      <Textarea
        id="market-upload-deps"
        aria-invalid={Boolean(props.errors.dependencies)}
        value={props.form.dependencies}
        onChange={(event) => props.update("dependencies", event.target.value)}
        placeholder="slug@1.0.0"
        className={cn(
          "min-h-16 font-mono text-xs",
          props.errors.dependencies && "border-destructive"
        )}
      />
      <p className="text-[10px] text-muted-foreground">
        {props.t("upload.dependenciesHint")}
      </p>
      <UploadFieldError t={props.t} error={props.errors.dependencies} />
    </div>
  )
}

function TagsField(props: UploadDetailsProps) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor="market-upload-tags">{props.t("upload.tags")}</Label>
      <Input
        id="market-upload-tags"
        value={props.form.tags}
        onChange={(event) => props.update("tags", event.target.value)}
        placeholder="tag1, tag2"
      />
    </div>
  )
}

export function UploadVersionFields(props: UploadDetailsProps) {
  return (
    <>
      <VersionField {...props} />
      <ChangelogField {...props} />
      <DependenciesField {...props} />
      {props.isPublish ? <TagsField {...props} /> : null}
    </>
  )
}
