import type { Extensions } from "@tiptap/core"
import { Placeholder } from "@tiptap/extension-placeholder"
import StarterKit from "@tiptap/starter-kit"

import { InactiveSelectionHighlight } from "./inactive-selection"
import { Reference } from "./nodes/reference-node"
import {
  MentionSuggestion,
  type MentionController,
} from "./suggestion/mention-suggestion"

/**
 * Options for the shared composer extension set.
 */
export interface ComposerExtensionOptions {
  /** Placeholder shown when the document is empty. */
  placeholder?: string
  /**
   * When provided, enables the unified `@` mention panel: the suggestion plugin
   * forwards lifecycle/keys to this controller, whose React popup owns data and
   * insertion.
   */
  mentionController?: MentionController
}

/**
 * Build the Tiptap extension set powering the rich-text composer.
 *
 * Shared by the live editor ({@link "./rich-composer".RichComposer}) and the
 * headless editor used in tests, so the Markdown round-trip exercised by tests
 * matches what users actually type.
 *
 * StarterKit (v3) already bundles paragraph/heading/lists/bold/italic/strike/
 * code/codeBlock/blockquote/link/history/hardBreak and the relevant input
 * rules, which gives us live WYSIWYG Markdown. `Markdown` adds
 * `editor.getMarkdown()` / `editor.markdown.parse()` for serialization.
 *
 * The bundled Link mark is kept (genuine `[label](uri)` markdown must still
 * round-trip — references downgraded to markdown, hydrated drafts, quick
 * messages), but every path that fabricates a link from plain text — or
 * navigates on click — is turned off, all wrong for a message composer:
 *  - `shouldAutoLink: () => false` is the key lever. linkifyjs wraps any
 *    domain-shaped token, so a filename like `lib.rs`, `notes.md` or `setup.io`
 *    gets linkified because its extension is a real TLD (`.rs`/`.md`/`.io`/
 *    `.sh`/`.py`/… match; `.ts`/`.tsx`/`.json` don't, which is why it only bit
 *    *some* files). That opens a browser on click AND silently rewrites the
 *    message to `[lib.rs](http://lib.rs)` on serialize — the reported bug.
 *    `shouldAutoLink` gates BOTH of linkifyjs's entry points: the
 *    autolink-on-type plugin and the always-installed paste rule
 *    (`addPasteRules`). That paste rule ignores `autolink`/`linkOnPaste` and
 *    consults only `shouldAutoLink`, so turning off `autolink` alone would
 *    still linkify a real *paste* — the exact reported action — which is why
 *    the gate, not just the flags, is required.
 *  - `autolink: false` / `linkOnPaste: false` additionally remove the
 *    autolink-on-type and paste-URL-onto-selection plugins outright.
 *  - `openOnClick: false`: a click inside the editor places the caret instead
 *    of navigating away.
 * None of this touches the `@tiptap/markdown` parser (built on `marked`, which
 * never consults `shouldAutoLink`). That parser runs only when *authored*
 * markdown is hydrated — drafts, queue-edits, quick messages, `setMarkdown` —
 * never when plain text is typed or pasted. It parses explicit `[label](uri)`
 * links, and GFM-autolinks a *real* bare URL / email (`https://…`, `www.…`,
 * `a@b.com`) but never a bare filename like `lib.rs`. So the reported file-path
 * bug is closed on every path, while a genuine URL in restored content still
 * renders as a (now non-navigating) link — intended, and consistent with how
 * it was authored.
 */
export function buildComposerExtensions(
  options: ComposerExtensionOptions = {}
): Extensions {
  const extensions: Extensions = [
    StarterKit.configure({
      blockquote: false,
      bold: false,
      bulletList: false,
      code: false,
      codeBlock: false,
      heading: false,
      horizontalRule: false,
      italic: false,
      link: false,
      listItem: false,
      listKeymap: false,
      orderedList: false,
      strike: false,
      underline: false,
    }),
    Placeholder.configure({
      placeholder: options.placeholder ?? "",
      // Only paint the placeholder while the editor is editable so a disabled
      // composer reads as empty rather than as a hint.
      showOnlyWhenEditable: true,
    }),
    Reference,
    // Keeps the selection visible when focus moves to the right-click menu.
    InactiveSelectionHighlight,
  ]
  if (options.mentionController) {
    extensions.push(
      MentionSuggestion.configure({ controller: options.mentionController })
    )
  }
  return extensions
}
