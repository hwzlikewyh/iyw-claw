Function IywClawEnsureDesktopShortcut
  StrCmp $NoShortcutMode "1" iyw_shortcut_done 0
  CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$IywClawAppDir\${MAINBINARYNAME}.exe"
  !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
  DetailPrint "已创建桌面快捷方式。"

  iyw_shortcut_done:
FunctionEnd
