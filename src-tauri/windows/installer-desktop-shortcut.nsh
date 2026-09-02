Function IywClawEnsureDesktopShortcut
  StrCmp $NoShortcutMode "1" iyw_shortcut_done 0
  ; Custom hooks are included after Tauri expands its template macros. Use the
  ; runtime language token and stable binary name instead of template macros,
  ; which otherwise become literal shortcut text.
  CreateShortcut "$DESKTOP\$(^Name).lnk" "$IywClawAppDir\iyw-claw.exe"
  !insertmacro SetLnkAppUserModelId "$DESKTOP\$(^Name).lnk"
  CreateDirectory "$SMPROGRAMS\$(^Name)"
  CreateShortcut "$SMPROGRAMS\$(^Name)\$(^Name).lnk" "$IywClawAppDir\iyw-claw.exe"
  !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\$(^Name)\$(^Name).lnk"
  DetailPrint "已创建桌面和开始菜单快捷方式。"

  iyw_shortcut_done:
FunctionEnd

Function un.IywClawRemoveShortcuts
  Delete "$DESKTOP\$(^Name).lnk"
  Delete "$SMPROGRAMS\$(^Name)\$(^Name).lnk"
  RMDir "$SMPROGRAMS\$(^Name)"
FunctionEnd
