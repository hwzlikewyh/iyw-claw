; Older builds stored the physical application directory in InstallRoot. The
; current layout stores the logical root and appends one \app directory. Use
; the executable location to distinguish the two forms before appending.
Var IywClawRequestedRoot
Var IywClawInstallDirectoryRestored

Function IywClawReadRequestedInstallRoot
  ; NSIS 会从 $CMDLINE 移除 /D=，必须读取 Windows 保留的原始命令行。
  StrCpy $IywClawRequestedRoot ""
  System::Call 'kernel32::GetCommandLineW() w.R6'
  ClearErrors
  ${GetOptions} $R6 "/D=" $R7
  IfErrors requested_root_read 0
  ; 完整路径由 NSIS 原生解析，避免重复解析空格、正斜杠等合法路径字符。
  GetFullPathName $IywClawRequestedRoot "$INSTDIR"
  requested_root_read:
FunctionEnd

Function IywClawRestoreInstallDirectory
  StrCpy $IywClawInstallDirectoryRestored "1"
  StrCmp $IywClawOriginalSid "" restore_requested_install_directory 0
  StrCpy $INSTDIR "$IywClawSelectedRoot"
  Return

  restore_requested_install_directory:
  Call IywClawReadRequestedInstallRoot
  StrCmp $IywClawRequestedRoot "" restore_stored_install_directory 0
  GetFullPathName $INSTDIR "$IywClawRequestedRoot"
  DetailPrint "使用显式安装目录：$INSTDIR"
  Return

  restore_stored_install_directory:
    ReadRegStr $R8 SHCTX "$IywClawInstallRegistryKey" "InstallRoot"
    StrCmp $R8 "" restore_default_install_directory 0
    ; 产品独立记录不随发布者变化，优先于 Tauri 按发布者推导的默认目录。
    GetFullPathName $INSTDIR "$R8"
    DetailPrint "沿用原安装目录：$INSTDIR"
    Return

  restore_default_install_directory:
    GetFullPathName $INSTDIR "$LOCALAPPDATA\iyw-claw"
FunctionEnd

Function IywClawSelectInstallRoot
  ; GUIINIT 已恢复过目录时保留目录页的新选择；静默模式补齐相同初始化。
  StrCmp $IywClawInstallDirectoryRestored "1" select_saved_install_root 0
  Call IywClawRecoverSmokeInstallRoot
  Call IywClawNormalizeLegacyInstallRoot
  Call IywClawRestoreInstallDirectory

  select_saved_install_root:
    ReadRegStr $R8 SHCTX "$IywClawInstallRegistryKey" "InstallRoot"
    StrCmp $R8 "" select_current_install_root 0
    GetFullPathName $R9 "$R8\app"
    GetFullPathName $R7 "$INSTDIR"
    StrCmp $R7 $R9 0 select_current_install_root

    StrCpy $IywClawRoot "$R8"
    DetailPrint "沿用原安装目录：$IywClawRoot"
    Return

  select_current_install_root:
    StrCpy $IywClawRoot "$INSTDIR"
FunctionEnd

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
    ; hooks 在 Tauri 定义 UNINSTKEY 前展开，使用固定且与发布者无关的兼容项。
    ReadRegStr $R8 SHCTX "${IYW_CLAW_UNINSTALL_REGISTRY_KEY}" "InstallLocation"
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
