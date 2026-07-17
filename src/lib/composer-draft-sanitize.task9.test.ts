import { describe, expect, it } from "vitest"

import { sanitizeComposerDraftDoc } from "@/lib/composer-draft-sanitize"

describe("sanitizeComposerDraftDoc", () => {
  it("flattens legacy rich blocks while preserving words and references", () => {
    const input = {
      type: "doc",
      content: [
        {
          type: "heading",
          content: [{ type: "text", text: "Title", marks: [{ type: "bold" }] }],
        },
        {
          type: "bulletList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [
                    {
                      type: "reference",
                      attrs: { refType: "file", id: "a.ts", label: "a.ts" },
                    },
                    { type: "text", text: " item" },
                  ],
                },
              ],
            },
          ],
        },
      ],
    }

    expect(sanitizeComposerDraftDoc(input)).toEqual({
      type: "doc",
      content: [
        { type: "paragraph", content: [{ type: "text", text: "Title" }] },
        {
          type: "paragraph",
          content: [
            {
              type: "reference",
              attrs: { refType: "file", id: "a.ts", label: "a.ts" },
            },
            { type: "text", text: " item" },
          ],
        },
      ],
    })
  })
})
