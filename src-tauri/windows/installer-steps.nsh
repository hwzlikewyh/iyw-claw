!macro IywClawCreateStep Index Label
  !define /math IYW_STEP_LABEL 2400 + ${Index}
  !define /math IYW_STEP_BAR 2410 + ${Index}
  System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w "0${Index}  ${Label}", i 0x50000000, i R3, i R5, i R4, i R6, p R0, p ${IYW_STEP_LABEL}, p 0, p 0) p.R1'
  SetCtlColors $R1 626C76 FFFFFF
  SendMessage $R1 ${WM_SETFONT} $IywClawStatusFont 1
  System::Call 'user32::CreateWindowExW(i 0, w "msctls_progress32", w "", i 0x50000001, i R3, i R7, i R4, i R8, p R0, p ${IYW_STEP_BAR}, p 0, p 0) p.R1'
  System::Call 'uxtheme::SetWindowTheme(p R1, w "", w "")'
  SendMessage $R1 0x406 0 100
  SendMessage $R1 0x409 0 0x00877E13
  SendMessage $R1 0x2001 0 0x00F0EEEB
  IntOp $R3 $R3 + $R4
  IntOp $R3 $R3 + $R9
  !undef IYW_STEP_LABEL
  !undef IYW_STEP_BAR
!macroend

Function IywClawCreateSteps
  FindWindow $R0 "#32770" "" $HWNDPARENT
  System::Call 'user32::GetDpiForWindow(p R0) i.R2'
  System::Call 'kernel32::MulDiv(i 12, i R2, i 96) i.R9'
  System::Call 'kernel32::MulDiv(i 24, i R2, i 96) i.R3'
  IntOp $R4 $R4 - $R9
  IntOp $R4 $R4 - $R9
  IntOp $R4 $R4 / 3
  System::Call 'kernel32::MulDiv(i 8, i R2, i 96) i.R5'
  System::Call 'kernel32::MulDiv(i 24, i R2, i 96) i.R6'
  System::Call 'kernel32::MulDiv(i 40, i R2, i 96) i.R7'
  System::Call 'kernel32::MulDiv(i 3, i R2, i 96) i.R8'
  !insertmacro IywClawCreateStep 1 "准备安装"
  !insertmacro IywClawCreateStep 2 "初始化组件"
  !insertmacro IywClawCreateStep 3 "完成设置"
FunctionEnd

!macro IywClawCompleteStep Index Label
  !define /math IYW_STEP_LABEL 2400 + ${Index}
  !define /math IYW_STEP_BAR 2410 + ${Index}
  GetDlgItem $R1 $R0 ${IYW_STEP_LABEL}
  SendMessage $R1 ${WM_SETTEXT} 0 "STR:✓  ${Label}"
  GetDlgItem $R1 $R0 ${IYW_STEP_BAR}
  SendMessage $R1 0x402 100 0
  !undef IYW_STEP_LABEL
  !undef IYW_STEP_BAR
!macroend

Function IywClawCompleteSteps
  FindWindow $R0 "#32770" "" $HWNDPARENT
  !insertmacro IywClawCompleteStep 1 "准备安装"
  !insertmacro IywClawCompleteStep 2 "初始化组件"
  !insertmacro IywClawCompleteStep 3 "完成设置"
FunctionEnd
