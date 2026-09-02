; Older builds stored the physical application directory in InstallRoot. The
; current layout stores the logical root and appends one \app directory. Use
; the executable location to distinguish the two forms before appending.
Function IywClawNormalizeLegacyInstallRoot
  ReadRegStr $R8 SHCTX "$IywClawInstallRegistryKey" "InstallRoot"
  StrCmp $R8 "" iyw_read_tauri_install_root 0
  IfFileExists "$R8\app\iyw-claw.exe" iyw_legacy_root_done 0
  IfFileExists "$R8\iyw-claw.exe" iyw_validate_legacy_app_dir iyw_legacy_root_done

  iyw_read_tauri_install_root:
    ReadRegStr $R8 SHCTX "${MANUPRODUCTKEY}" ""
    StrCmp $R8 "" iyw_legacy_root_done 0
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
