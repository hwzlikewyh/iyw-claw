; Older builds stored the physical application directory in InstallRoot. The
; current layout stores the logical root and appends one \app directory. Use
; the executable location to distinguish the two forms before appending.
Function IywClawIsSmokeInstallRoot
  Exch $R8
  Push $R7
  Push $R9
  GetFullPathName $R9 "$TEMP"
  StrCpy $R9 "$R9\iyw-claw-nsis-smoke-"
  StrLen $R7 $R9
  StrCpy $R8 $R8 $R7
  StrCmp $R8 $R9 0 smoke_root_not_found
  StrCpy $R8 "1"
  Goto smoke_root_checked
  smoke_root_not_found:
    StrCpy $R8 "0"
  smoke_root_checked:
    Pop $R9
    Pop $R7
    Exch $R8
FunctionEnd

Function IywClawRecoverSmokeInstallRoot
  StrCmp $IywClawInstallerTestMode "1" smoke_recovery_done
  ReadRegStr $R8 SHCTX "$IywClawInstallRegistryKey" "InstallRoot"
  Push "$R8"
  Call IywClawIsSmokeInstallRoot
  Pop $R5
  StrCmp $R5 "1" 0 smoke_recovery_done
  ReadRegStr $R8 SHCTX "$IywClawInstallRegistryKey" ""
  Push "$R8"
  Call IywClawIsSmokeInstallRoot
  Pop $R5
  StrCmp $R5 "1" smoke_recovery_default
  IfFileExists "$R8\app\iyw-claw.exe" 0 smoke_recovery_default
  WriteRegStr SHCTX "$IywClawInstallRegistryKey" "InstallRoot" "$R8"
  DetailPrint "已恢复原安装目录：$R8"
  Return
  smoke_recovery_default:
    WriteRegStr SHCTX "$IywClawInstallRegistryKey" "InstallRoot" ""
    DetailPrint "测试目录记录已失效，将使用正常安装目录。"
  smoke_recovery_done:
FunctionEnd

Function IywClawNormalizeLegacyInstallRoot
  ReadRegStr $R8 SHCTX "$IywClawInstallRegistryKey" "InstallRoot"
  StrCmp $R8 "" iyw_read_tauri_install_root 0
  IfFileExists "$R8\app\iyw-claw.exe" iyw_legacy_root_done 0
  IfFileExists "$R8\iyw-claw.exe" iyw_validate_legacy_app_dir iyw_read_tauri_install_root

  iyw_read_tauri_install_root:
    ; Tauri writes the physical application directory to the uninstall key.
    ; The default value in MANUPRODUCTKEY is not the install location.
    StrCmp $IywClawInstallerTestMode "1" iyw_legacy_root_done 0
    ReadRegStr $R8 SHCTX "${UNINSTKEY}" "InstallLocation"
    StrCmp $R8 "" iyw_legacy_root_done 0
    ; The uninstall registry value is commonly quoted when it contains spaces.
    StrCpy $R6 $R8 1
    StrCmp $R6 '"' 0 iyw_standard_path_unquoted
    StrCpy $R8 $R8 "" 1
    StrLen $R7 $R8
    IntOp $R7 $R7 - 1
    StrCpy $R6 $R8 1 $R7
    StrCmp $R6 '"' 0 iyw_standard_path_unquoted
    StrCpy $R8 $R8 $R7

  iyw_standard_path_unquoted:
    Push "$R8"
    Call IywClawIsSmokeInstallRoot
    Pop $R5
    StrCmp $R5 "1" iyw_legacy_root_done
    IfFileExists "$R8\iyw-claw.exe" 0 iyw_legacy_root_done

  iyw_validate_legacy_app_dir:
    GetFullPathName $R9 "$R8\.."
    GetFullPathName $R7 "$R9\app"
    GetFullPathName $R6 "$R8"
    StrCmp $R7 $R6 0 iyw_legacy_root_done
    WriteRegStr SHCTX "$IywClawInstallRegistryKey" "InstallRoot" "$R9"
    DetailPrint "已迁移旧版安装目录：$R9"

  iyw_legacy_root_done:
FunctionEnd
