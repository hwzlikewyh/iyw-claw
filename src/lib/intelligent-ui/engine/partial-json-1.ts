// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。

// Lenient JSON reader for interactive blocks that are still being written.
//
// Models stream a block token by token and routinely deviate from strict JSON:
// trailing commas, // and /* */ comments, unescaped double quotes inside text
// (typical when Chinese prose uses ASCII quotes). This reader accepts all of
// that and any prefix of a document, and reports what was actually finished:
//
//   - `closed` holds every object and array whose closing bracket was read, so a
//     renderer can wait for a leaf node to finish while showing its container.
//   - `openTail` maps a container that was cut off to the key (or index) of its
//     last member when that member itself was cut off mid-value.
//
// A quote inside a string ends the string only when the next significant
// character is structural (`,` `}` `]` `:`) or the input ends there.

export type PartialJSONResult = {
  value: unknown
  // False when the input ended before the top-level value was finished.
  complete: boolean
  closed: WeakSet<object>
  openTail: WeakMap<object, string | number>
}

// Far deeper than any block allows (the block limit is 8 levels of nodes); a
// hard stop keeps hostile input from exhausting the call stack.
export const MAX_JSON_DEPTH = 64

export const STRUCTURAL_AFTER_STRING = new Set([",", "}", "]", ":"])
