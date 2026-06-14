; AnyPlug Windows Installer — NSIS script
; Creates a portable installer for the AnyPlug USB/IP passthrough service + GUI.
; ----------------------------------------------------------------------------

!define PRODUCT_NAME "AnyPlug"
!define PRODUCT_VERSION "0.1.0"
!define PRODUCT_PUBLISHER "AnyPlug Contributors"
!define PRODUCT_WEB_SITE "https://github.com/stanvx/AnyPlug"

SetCompressor lzma

; Modern UI
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"

; ----------------------------------------
; Installer attributes
; ----------------------------------------
Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "AnyPlug-${PRODUCT_VERSION}-setup.exe"
InstallDir "$PROGRAMFILES64\${PRODUCT_NAME}"
InstallDirRegKey HKLM "Software\${PRODUCT_NAME}" ""
RequestExecutionLevel admin

; ----------------------------------------
; Version info
; ----------------------------------------
VIProductVersion "0.1.0.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "FileVersion" "${PRODUCT_VERSION}"
VIAddVersionKey "ProductVersion" "${PRODUCT_VERSION}"
VIAddVersionKey "FileDescription" "AnyPlug USB/IP Passthrough"
VIAddVersionKey "LegalCopyright" "Copyright (c) 2026 AnyPlug Contributors"
VIAddVersionKey "CompanyName" "${PRODUCT_PUBLISHER}"

; ----------------------------------------
; Interface settings
; ----------------------------------------
!define MUI_ABORTWARNING
!define MUI_ICON "assets\anyplug.ico"
!define MUI_UNICON "assets\anyplug.ico"

; ----------------------------------------
; Pages
; ----------------------------------------
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "../../LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; ----------------------------------------
; Languages
; ----------------------------------------
!insertmacro MUI_LANGUAGE "English"

; ----------------------------------------
; Installation sections
; ----------------------------------------
Section "AnyPlug (required)" SecMain
  SectionIn RO

  SetOutPath "$INSTDIR"

  ; Main binary
  File "target\x86_64-pc-windows-gnu\release\anyplug.exe"

  ; Service helper — install/remove scripts
  FileOpen $0 "$INSTDIR\install-service.bat" w
  FileWrite $0 '@echo off$\r$\n'
  FileWrite $0 'sc create AnyPlug binPath="$INSTDIR\anyplug.exe service" start=auto$\r$\n'
  FileWrite $0 'sc description AnyPlug "AnyPlug USB/IP passthrough service"$\r$\n'
  FileWrite $0 'echo Service installed. Start with: sc start AnyPlug$\r$\n'
  FileClose $0

  FileOpen $1 "$INSTDIR\remove-service.bat" w
  FileWrite $1 '@echo off$\r$\n'
  FileWrite $1 'sc stop AnyPlug$\r$\n'
  FileWrite $1 'sc delete AnyPlug$\r$\n'
  FileWrite $1 'echo Service removed.$\r$\n'
  FileClose $1

  ; Write uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Start menu shortcuts
  CreateDirectory "$SMPROGRAMS\AnyPlug"
  CreateShortCut "$SMPROGRAMS\AnyPlug\AnyPlug.lnk" "$INSTDIR\anyplug.exe" "" "$INSTDIR\anyplug.exe" 0
  CreateShortCut "$SMPROGRAMS\AnyPlug\Uninstall.lnk" "$INSTDIR\uninstall.exe"
  CreateShortCut "$SMPROGRAMS\AnyPlug\Install Service.lnk" "$INSTDIR\install-service.bat"
  CreateShortCut "$SMPROGRAMS\AnyPlug\Remove Service.lnk" "$INSTDIR\remove-service.bat"

  ; Registry for Add/Remove Programs
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
                   "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
                   "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
                   "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
                   "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
                     "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
                     "NoRepair" 1
SectionEnd

; ----------------------------------------
; Uninstaller
; ----------------------------------------
Section "Uninstall"
  ; Stop and remove service
  ExecWait '"$INSTDIR\anyplug.exe" service --stop'
  Sleep 1000
  ExecWait 'sc delete AnyPlug'

  ; Remove files
  Delete "$INSTDIR\anyplug.exe"
  Delete "$INSTDIR\install-service.bat"
  Delete "$INSTDIR\remove-service.bat"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  ; Remove shortcuts
  Delete "$SMPROGRAMS\AnyPlug\AnyPlug.lnk"
  Delete "$SMPROGRAMS\AnyPlug\Uninstall.lnk"
  Delete "$SMPROGRAMS\AnyPlug\Install Service.lnk"
  Delete "$SMPROGRAMS\AnyPlug\Remove Service.lnk"
  RMDir "$SMPROGRAMS\AnyPlug"

  ; Remove registry keys
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
  DeleteRegKey HKLM "Software\${PRODUCT_NAME}"
SectionEnd
