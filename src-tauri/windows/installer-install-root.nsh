; Older builds stored the physical application directory in InstallRoot. The
; current layout stores the logical root and appends one \app directory. Use
; the executable location to distinguish the two forms before appending.
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
