!macro IywClawSetBrandShortcutId Shortcut
  !ifdef BUNDLEID
    !define IYW_CLAW_SAVED_BUNDLE_ID "${BUNDLEID}"
  !endif
  !undef /noerrors BUNDLEID
  !define BUNDLEID "$IywClawBrandIdentifier"
  !insertmacro SetLnkAppUserModelId "${Shortcut}"
  !undef BUNDLEID
  !ifdef IYW_CLAW_SAVED_BUNDLE_ID
    !define BUNDLEID "${IYW_CLAW_SAVED_BUNDLE_ID}"
    !undef IYW_CLAW_SAVED_BUNDLE_ID
  !endif
!macroend

Function IywClawEnsureDesktopShortcut
  StrCmp $IywClawBrandNoShortcut "1" iyw_shortcut_done 0
  ; Custom hooks are included after Tauri expands its template macros. Use the
  ; runtime language token and stable binary name instead of template macros,
  ; which otherwise become literal shortcut text.
  Delete "$DESKTOP\$IywClawBrandName.lnk"
  CreateShortcut "$DESKTOP\$IywClawBrandName.lnk" "$IywClawAppDir\iyw-claw.exe" "" "$IywClawBrandIcon"
  !insertmacro IywClawSetBrandShortcutId "$DESKTOP\$IywClawBrandName.lnk"
  CreateDirectory "$SMPROGRAMS\$IywClawBrandName"
  CreateShortcut "$SMPROGRAMS\$IywClawBrandName\$IywClawBrandName.lnk" "$IywClawAppDir\iyw-claw.exe" "" "$IywClawBrandIcon"
  !insertmacro IywClawSetBrandShortcutId "$SMPROGRAMS\$IywClawBrandName\$IywClawBrandName.lnk"
  DetailPrint "已创建桌面和开始菜单快捷方式。"

  iyw_shortcut_done:
FunctionEnd

Function un.IywClawRemoveShortcuts
  StrCmp $UpdateMode "1" brand_uninstall_links_done
  Delete "$DESKTOP\$IywClawBrandName.lnk"
  Delete "$SMPROGRAMS\$IywClawBrandName\$IywClawBrandName.lnk"
  RMDir "$SMPROGRAMS\$IywClawBrandName"
  DeleteRegKey SHCTX "Software\Classes\$IywClawBrandScheme"
  brand_uninstall_links_done:
FunctionEnd
