Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
!include "LogicLib.nsh"

Name "${INSTANCE_LABEL}"
VIProductVersion "1.0.0.0"
VIAddVersionKey /LANG=2052 "ProductName" "${INSTANCE_LABEL}"
VIAddVersionKey /LANG=2052 "FileDescription" "${INSTANCE_LABEL}"
VIAddVersionKey /LANG=2052 "FileVersion" "${PLATFORM_VERSION}"
VIAddVersionKey /LANG=2052 "ProductVersion" "${PLATFORM_VERSION}"
OutFile "yaya-instance-installer.exe"
InstallDir "$LOCALAPPDATA\Yaya\Instances\${INSTANCE_ID}"
RequestExecutionLevel user
SetCompressor lzma

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Yaya-${INSTANCE_ID}"
!define MUI_FINISHPAGE_RUN "$INSTDIR\yaya-desktop.exe"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "此安装包需要支持 x64 应用的 Windows 系统。"
    Abort
  ${EndIf}
FunctionEnd

Section "安装"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "yaya-desktop.exe"
  File "instance-url.txt"
  File "platform-package.json"
  CreateShortcut "$DESKTOP\${INSTANCE_LABEL}.lnk" "$INSTDIR\yaya-desktop.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${INSTANCE_LABEL}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${PLATFORM_VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\yaya-desktop.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '$\"$INSTDIR\uninstall.exe$\"'
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$DESKTOP\${INSTANCE_LABEL}.lnk"
  Delete "$INSTDIR\yaya-desktop.exe"
  Delete "$INSTDIR\instance-url.txt"
  Delete "$INSTDIR\platform-package.json"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
