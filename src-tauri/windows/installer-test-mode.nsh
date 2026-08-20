!define IYW_CLAW_INSTALL_TEST_REGISTRY_KEY_PREFIX "Software\iywclaw\iyw-claw-installer-test"

Var IywClawInstallerTestMode
Var IywClawInstallerTestId

!macro IywClawDefineTestIdValidationFunction Prefix
Function ${Prefix}IywClawIsValidTestId
  Pop $R6
  StrLen $R7 $R6
  StrCmp $R7 "32" 0 iyw_invalid_test_id
  StrCpy $R7 "0"

  iyw_test_id_character_loop:
    IntCmp $R7 32 iyw_valid_test_id iyw_test_id_character iyw_invalid_test_id

  iyw_test_id_character:
    StrCpy $R8 $R6 1 $R7
    StrCmp $R8 "0" iyw_test_id_next
    StrCmp $R8 "1" iyw_test_id_next
    StrCmp $R8 "2" iyw_test_id_next
    StrCmp $R8 "3" iyw_test_id_next
    StrCmp $R8 "4" iyw_test_id_next
    StrCmp $R8 "5" iyw_test_id_next
    StrCmp $R8 "6" iyw_test_id_next
    StrCmp $R8 "7" iyw_test_id_next
    StrCmp $R8 "8" iyw_test_id_next
    StrCmp $R8 "9" iyw_test_id_next
    StrCmp $R8 "a" iyw_test_id_next
    StrCmp $R8 "b" iyw_test_id_next
    StrCmp $R8 "c" iyw_test_id_next
    StrCmp $R8 "d" iyw_test_id_next
    StrCmp $R8 "e" iyw_test_id_next
    StrCmp $R8 "f" iyw_test_id_next
    Goto iyw_invalid_test_id

  iyw_test_id_next:
    IntOp $R7 $R7 + 1
    Goto iyw_test_id_character_loop

  iyw_valid_test_id:
    Push "1"
    Return

  iyw_invalid_test_id:
    Push "0"
FunctionEnd
!macroend

!macro IywClawDefineInstallerModeFunction Prefix
Function ${Prefix}IywClawConfigureInstallerMode
  StrCpy $IywClawInstallRegistryKey "${IYW_CLAW_INSTALL_REGISTRY_KEY}"
  StrCpy $IywClawInstallerTestMode "0"
  StrCpy $IywClawInstallerTestId ""
  ClearErrors
  ${GetOptions} $CMDLINE "/IYW_CLAW_TEST_MODE=" $R6
  IfErrors iyw_installer_mode_done
  StrCpy $IywClawInstallerTestMode "invalid"
  Push $R6
  Call ${Prefix}IywClawIsValidTestId
  Pop $R7
  StrCmp $R7 "1" 0 iyw_installer_mode_done
  StrCpy $IywClawInstallRegistryKey "${IYW_CLAW_INSTALL_TEST_REGISTRY_KEY_PREFIX}\$R6"
  StrCpy $IywClawInstallerTestMode "1"
  StrCpy $IywClawInstallerTestId "$R6"

  iyw_installer_mode_done:
FunctionEnd
!macroend

!macro IywClawDefineTestRootValidationFunction Prefix
Function ${Prefix}IywClawValidateTestRoot
  StrCmp $IywClawInstallerTestMode "1" 0 iyw_invalid_test_root
  StrCmp $IywClawInstallerTestId "" iyw_invalid_test_root 0
  GetFullPathName $R6 "$TEMP"
  StrCpy $R8 "$R6\iyw-claw-nsis-smoke-$IywClawInstallerTestId"
  StrCmp $IywClawRoot $R8 iyw_valid_test_root iyw_invalid_test_root

  iyw_valid_test_root:
    Push "1"
    Return

  iyw_invalid_test_root:
    Push "0"
FunctionEnd
!macroend

!insertmacro IywClawDefineTestIdValidationFunction ""
!insertmacro IywClawDefineTestIdValidationFunction "un."
!insertmacro IywClawDefineInstallerModeFunction ""
!insertmacro IywClawDefineInstallerModeFunction "un."
!insertmacro IywClawDefineTestRootValidationFunction ""
!insertmacro IywClawDefineTestRootValidationFunction "un."
