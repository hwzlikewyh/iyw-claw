Function IywClawEnsureDesktopShortcut
  StrCmp $NoShortcutMode "1" iyw_shortcut_done 0
  ; Custom hooks are included after Tauri expands its template macros. Use the
  ; runtime language token and stable binary name instead of template macros,
  ; which otherwise become literal shortcut text.
  ; 桌面快捷方式由应用进程设置 AppUserModelID，避免写入未展开的占位符。
  Delete "$DESKTOP\$(^Name).lnk"
  CreateShortcut "$DESKTOP\$(^Name).lnk" "$IywClawAppDir\iyw-claw.exe" "" "$IywClawAppDir\iyw-claw.exe" 0
  CreateDirectory "$SMPROGRAMS\$(^Name)"
  CreateShortcut "$SMPROGRAMS\$(^Name)\$(^Name).lnk" "$IywClawAppDir\iyw-claw.exe" "" "$IywClawAppDir\iyw-claw.exe" 0
  !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\$(^Name)\$(^Name).lnk"
  DetailPrint "已创建桌面和开始菜单快捷方式。"

  iyw_shortcut_done:
FunctionEnd

Function un.IywClawRemoveShortcuts
  Delete "$DESKTOP\$(^Name).lnk"
  Delete "$SMPROGRAMS\$(^Name)\$(^Name).lnk"
  RMDir "$SMPROGRAMS\$(^Name)"
FunctionEnd
