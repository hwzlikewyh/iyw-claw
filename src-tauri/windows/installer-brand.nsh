Var IywClawBrandId
Var IywClawBrandName
Var IywClawBrandIdentifier
Var IywClawBrandScheme
Var IywClawBrandIcon
Var IywClawBrandUserDirectory
Var IywClawBrandUninstallKey
Var IywClawBrandNoShortcut

!macro IywClawBrandReader Prefix
Function ${Prefix}IywClawReadBrand
  InitPluginsDir
  File /oname=$PLUGINSDIR\iyw-brand.ps1 "${__FILEDIR__}\installer-brand.ps1"
  nsExec::ExecToStack /TIMEOUT=10000 '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\iyw-brand.ps1" -Root "$IywClawRoot" -Output "$PLUGINSDIR\iyw-brand.ini"'
  Pop $R0
  Pop $R1
  StrCmp $R0 "0" +3
    DetailPrint "读取安装品牌配置失败：$R1"
    Abort
  ReadINIStr $IywClawBrandId "$PLUGINSDIR\iyw-brand.ini" "Brand" "Id"
  ReadINIStr $IywClawBrandName "$PLUGINSDIR\iyw-brand.ini" "Brand" "Name"
  ReadINIStr $IywClawBrandIdentifier "$PLUGINSDIR\iyw-brand.ini" "Brand" "Identifier"
  ReadINIStr $IywClawBrandScheme "$PLUGINSDIR\iyw-brand.ini" "Brand" "Scheme"
  ReadINIStr $IywClawBrandIcon "$PLUGINSDIR\iyw-brand.ini" "Brand" "Icon"
  ReadINIStr $IywClawBrandUserDirectory "$PLUGINSDIR\iyw-brand.ini" "Brand" "UserDirectory"
  ReadINIStr $IywClawBrandUninstallKey "$PLUGINSDIR\iyw-brand.ini" "Brand" "UninstallKey"
  StrCmp $IywClawInstallerTestMode "1" brand_test_registry brand_regular_registry
  brand_test_registry:
    StrCpy $IywClawBrandName "iyw-claw-installer-test-$IywClawInstallerTestId"
    StrCpy $IywClawBrandIdentifier "app.iywclaw.test.$IywClawInstallerTestId"
    StrCpy $IywClawBrandUninstallKey "$IywClawInstallRegistryKey\Uninstall"
    StrCpy $IywClawBrandScheme "iyw-installer-test-$IywClawInstallerTestId"
    Goto brand_registry_ready
  brand_regular_registry:
    ReadINIStr $IywClawInstallRegistryKey "$PLUGINSDIR\iyw-brand.ini" "Brand" "InstallKey"
  brand_registry_ready:
  ReadEnvStr $R0 "IYW_CLAW_BRAND_ID"
  StrCmp $R0 $IywClawBrandId brand_environment_ready
    System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_CLAW_BRAND_HOME", w "$PROFILE\$IywClawBrandUserDirectory")'
  brand_environment_ready:
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_CLAW_BRAND_ID", w "$IywClawBrandId")'
FunctionEnd
!macroend

!insertmacro IywClawBrandReader ""
!insertmacro IywClawBrandReader "un."
