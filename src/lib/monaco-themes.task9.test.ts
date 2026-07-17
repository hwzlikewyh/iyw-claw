import { describe, expect, it, vi } from "vitest"

import {
  configureLanguageValidation,
  MONACO_UNICODE_HIGHLIGHT_OPTIONS,
} from "@/lib/monaco-themes"

describe("Monaco embedded editor policy", () => {
  it("keeps visible CJK text unboxed while preserving invisible checks", () => {
    expect(MONACO_UNICODE_HIGHLIGHT_OPTIONS).toEqual({
      ambiguousCharacters: false,
      nonBasicASCII: false,
    })
    expect(MONACO_UNICODE_HIGHLIGHT_OPTIONS.invisibleCharacters).toBeUndefined()
  })

  it("disables project-dependent diagnostics but keeps syntax validation", () => {
    const setTsCompiler = vi.fn()
    const setJsCompiler = vi.fn()
    const setTsDiagnostics = vi.fn()
    const setJsDiagnostics = vi.fn()
    const setJsonDiagnostics = vi.fn()
    const monaco = {
      languages: {
        typescript: {
          JsxEmit: { Preserve: 1 },
          ScriptTarget: { ESNext: 99 },
          ModuleKind: { ESNext: 99 },
          ModuleResolutionKind: { NodeJs: 2 },
          typescriptDefaults: {
            setCompilerOptions: setTsCompiler,
            setDiagnosticsOptions: setTsDiagnostics,
          },
          javascriptDefaults: {
            setCompilerOptions: setJsCompiler,
            setDiagnosticsOptions: setJsDiagnostics,
          },
        },
        json: {
          jsonDefaults: { setDiagnosticsOptions: setJsonDiagnostics },
        },
      },
    }

    configureLanguageValidation(monaco as never)

    expect(setTsCompiler).toHaveBeenCalledOnce()
    expect(setJsCompiler).toHaveBeenCalledOnce()
    expect(setTsDiagnostics).toHaveBeenCalledWith({
      noSemanticValidation: true,
      noSyntaxValidation: false,
      noSuggestionDiagnostics: true,
    })
    expect(setJsDiagnostics).toHaveBeenCalledOnce()
    expect(setJsonDiagnostics).toHaveBeenCalledWith(
      expect.objectContaining({
        validate: true,
        enableSchemaRequest: false,
        schemaRequest: "ignore",
        schemaValidation: "ignore",
      })
    )
  })
})
