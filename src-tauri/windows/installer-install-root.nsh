; Older builds stored the physical application directory in InstallRoot. The
; current layout stores the logical root and appends one \app directory. Use
; the executable location to distinguish the two forms before appending.
Var IywClawRequestedRoot
Var IywClawInstallDirectoryRestored

!macro IywClawGetFullInstallPath Output Input
  Push "${Input}"
  Call IywClawGetFullInstallPath
  Pop ${Output}
!macroend

Function IywClawGetFullInstallPath
  Exch $R0
  Push $R1
  Push $R2
  ; NSIS GetFullPathName 会拒绝尚不存在的目录；安装前只解析路径。
  StrLen $R1 $R0
  IntCmp $R1 2 install_path_unquote install_path_resolve install_path_unquote
  install_path_unquote:
    StrCpy $R2 $R0 1
    StrCmp $R2 '"' 0 install_path_resolve
    StrCpy $R2 $R0 1 -1
    StrCmp $R2 '"' 0 install_path_resolve
    IntOp $R1 $R1 - 2
    StrCpy $R0 $R0 $R1 1
  install_path_resolve:
    StrCmp $R0 "" install_path_invalid
    System::Call 'kernel32::GetFullPathNameW(w R0, i ${NSIS_MAX_STRLEN}, w .R1, p 0) i.R2'
    StrCmp $R2 "0" install_path_invalid
    IntCmp $R2 ${NSIS_MAX_STRLEN} install_path_invalid install_path_valid install_path_invalid
  install_path_valid:
    StrCpy $R0 $R1
    ClearErrors
    Goto install_path_done
  install_path_invalid:
    StrCpy $R0 ""
    SetErrors
  install_path_done:
    Pop $R2
    Pop $R1
    Exch $R0
FunctionEnd

Function IywClawReadStoredInstallRoot
  ReadRegStr $R8 SHCTX "$IywClawInstallRegistryKey" "InstallRoot"
  StrCmp $R8 "" stored_root_read
  !insertmacro IywClawGetFullInstallPath $R0 "$R8"
  IfErrors stored_root_read
  StrCpy $R8 $R0
  stored_root_read:
FunctionEnd

Function IywClawNormalizeSelectedInstallRoot
  ClearErrors
  !insertmacro IywClawGetFullInstallPath $R0 "$IywClawRoot"
  IfErrors selected_root_invalid
  StrCmp $R0 "" selected_root_invalid 0
  StrCpy $IywClawRoot "$R0"
  Return
  selected_root_invalid:
    StrCpy $IywClawTransactionError "安装目录无效，无法转换为绝对路径。请重新选择安装目录。"
    DetailPrint "$IywClawTransactionError"
    IfSilent selected_root_abort 0
    MessageBox MB_OK|MB_ICONSTOP "$IywClawTransactionError"
  selected_root_abort:
    SetErrorLevel 1
    Abort
FunctionEnd

Function IywClawReadRequestedInstallRoot
  ; NSIS 会从 $CMDLINE 移除 /D=，必须读取 Windows 保留的原始命令行。
  StrCpy $IywClawRequestedRoot ""
  System::Call 'kernel32::GetCommandLineW() w.R6'
  ClearErrors
  ${GetOptions} $R6 "/D=" $R7
  IfErrors requested_root_read 0
  ; /D 参数由 NSIS 原生解析，路径规范化不得要求目标目录已经存在。
  !insertmacro IywClawGetFullInstallPath $IywClawRequestedRoot "$INSTDIR"
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
  !insertmacro IywClawGetFullInstallPath $INSTDIR "$IywClawRequestedRoot"
  DetailPrint "使用显式安装目录：$INSTDIR"
  Return

  restore_stored_install_directory:
    Call IywClawReadStoredInstallRoot
    StrCmp $R8 "" restore_default_install_directory 0
    ; 产品独立记录不随发布者变化，优先于 Tauri 按发布者推导的默认目录。
    !insertmacro IywClawGetFullInstallPath $INSTDIR "$R8"
    DetailPrint "沿用原安装目录：$INSTDIR"
    Return

  restore_default_install_directory:
    !insertmacro IywClawGetFullInstallPath $INSTDIR "$LOCALAPPDATA\iyw-claw"
FunctionEnd

Function IywClawSelectInstallRoot
  ; GUIINIT 已恢复过目录时保留目录页的新选择；静默模式补齐相同初始化。
  StrCmp $IywClawInstallDirectoryRestored "1" select_saved_install_root 0
  Call IywClawRecoverSmokeInstallRoot
  Call IywClawNormalizeLegacyInstallRoot
  Call IywClawRestoreInstallDirectory

  select_saved_install_root:
    Call IywClawReadStoredInstallRoot
    StrCmp $R8 "" select_current_install_root 0
    ClearErrors
    !insertmacro IywClawGetFullInstallPath $R9 "$R8\app"
    IfErrors select_current_install_root
    !insertmacro IywClawGetFullInstallPath $R7 "$INSTDIR"
    IfErrors select_current_install_root
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
  !insertmacro IywClawGetFullInstallPath $R9 "$TEMP"
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
  Call IywClawReadStoredInstallRoot
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
  Call IywClawReadStoredInstallRoot
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
    !insertmacro IywClawGetFullInstallPath $R9 "$R8\.."
    !insertmacro IywClawGetFullInstallPath $R7 "$R9\app"
    !insertmacro IywClawGetFullInstallPath $R6 "$R8"
    StrCmp $R7 $R6 0 iyw_legacy_root_done
    WriteRegStr SHCTX "$IywClawInstallRegistryKey" "InstallRoot" "$R9"
    DetailPrint "已迁移旧版安装目录：$R9"

  iyw_legacy_root_done:
FunctionEnd
