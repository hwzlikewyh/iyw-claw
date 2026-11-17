Function IywClawEnsureDesktopShortcut
  StrCmp $NoShortcutMode "1" iyw_shortcut_done 0
  Delete "$DESKTOP\$(^Name).lnk"
  CreateShortcut "$DESKTOP\$(^Name).lnk" "$IywClawAppDir\iyw-claw.exe" "" "$IywClawAppDir\iyw-claw.exe" 0
  !insertmacro SetLnkAppUserModelId "$DESKTOP\$(^Name).lnk"
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
