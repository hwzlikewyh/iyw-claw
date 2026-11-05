Function IywClawRegisterNativeShare
  StrCmp $IywClawInstallerTestMode "1" native_share_install_done 0
  IfFileExists "$IywClawAppDir\native-share\IywAssistant.ShareTarget.msix" 0 native_share_install_done
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "$IywClawAppDir\native-share\register.ps1"'
  Pop $R0
  StrCmp $R0 "0" native_share_install_done 0
  DetailPrint "系统分享入口尚未注册（退出码=$R0），可在原助理的微信聊天记录窗口重试启用。"
  native_share_install_done:
FunctionEnd

Function un.IywClawUnregisterNativeShare
  StrCmp $IywClawInstallerTestMode "1" native_share_uninstall_done 0
  IfFileExists "$INSTDIR\native-share\register.ps1" 0 native_share_uninstall_done
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "$INSTDIR\native-share\register.ps1" -Unregister'
  Pop $R0
  StrCmp $R0 "0" native_share_uninstall_done 0
  MessageBox MB_OK|MB_ICONSTOP "无法移除系统分享入口（退出码=$R0），卸载尚未删除应用文件。请关闭微信后重试。"
  Abort
  native_share_uninstall_done:
FunctionEnd

Function un.IywClawPurgeNativeShare
  StrCmp $IywClawInstallerTestMode "1" native_share_purge_done 0
  IfFileExists "$INSTDIR\native-share\register.ps1" 0 native_share_purge_done
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "$INSTDIR\native-share\register.ps1" -PurgeInbox'
  Pop $R0
  StrCmp $R0 "0" native_share_purge_done 0
  MessageBox MB_OK|MB_ICONSTOP "无法清理微信分享记录（退出码=$R0），请关闭原助理后重试。"
  Abort
  native_share_purge_done:
FunctionEnd
