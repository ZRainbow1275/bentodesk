Unicode true
RequestExecutionLevel user
ManifestDPIAware true
SetCompressor /SOLID lzma
SetCompressorDictSize 32
CRCCheck on

!ifndef VERSION
  !error "VERSION is required"
!endif
!ifndef APP_EXE
  !error "APP_EXE is required"
!endif
!ifndef OUT_FILE
  !error "OUT_FILE is required"
!endif
!ifndef SOURCE_ROOT
  !error "SOURCE_ROOT is required"
!endif

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "nsDialogs.nsh"
!include "WinMessages.nsh"
!include "x64.nsh"

!define PRODUCT_NAME "BentoDesk"
!define PRODUCT_PUBLISHER "方寒"
!define PRODUCT_WEB_SITE "https://github.com/ZRainbow1275"
!define PRODUCT_X_SITE "https://x.com/zrainbo"
!define PRODUCT_EMAIL "hybridrevis@gmail.com"
!define PRODUCT_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\BentoDesk"
!define PRODUCT_REG_KEY "Software\BentoDesk"
!define LEGAL_REVISION "2026-08-18"
!define PURGE_CONFIRM_TOKEN "DELETE-BENTODESK-LOCAL-STATE"
!define APP_ICON "${SOURCE_ROOT}\crates\bentodesk-shell\app-icon.ico"

Name "${PRODUCT_NAME} ${VERSION}"
OutFile "${OUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\BentoDesk"
InstallDirRegKey HKCU "${PRODUCT_REG_KEY}" "InstallDir"
BrandingText "BentoDesk · Native · Local-first"
ShowInstDetails show
ShowUninstDetails show
SetFont "Microsoft YaHei UI" 9

VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "BentoDesk"
VIAddVersionKey /LANG=1033 "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "FileDescription" "BentoDesk Setup"
VIAddVersionKey /LANG=1033 "FileVersion" "${VERSION}.0"
VIAddVersionKey /LANG=1033 "CompanyName" "方寒"
VIAddVersionKey /LANG=1033 "LegalCopyright" "Copyright © 2026 方寒"

!define MUI_ICON "${APP_ICON}"
!define MUI_UNICON "${APP_ICON}"
!define MUI_WELCOMEFINISHPAGE_BITMAP "${SOURCE_ROOT}\\installer\\assets\\welcome.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "${SOURCE_ROOT}\\installer\\assets\\welcome.bmp"
!define MUI_WELCOMEFINISHPAGE_BITMAP_STRETCH "FitControl"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP_STRETCH "FitControl"
!define MUI_ABORTWARNING
!define MUI_UNABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\BentoDesk.exe"
!define MUI_FINISHPAGE_RUN_TEXT "$(RunBentoDesk)"
!define MUI_FINISHPAGE_LINK "$(FinishLinkText)"
!define MUI_FINISHPAGE_LINK_LOCATION "${PRODUCT_WEB_SITE}"
!define MUI_WELCOMEPAGE_TITLE "BentoDesk ${VERSION}"
!define MUI_WELCOMEPAGE_TEXT "$(WelcomeText)"
!define MUI_FINISHPAGE_TEXT "$(FinishText)"
!define MUI_LANGDLL_REGISTRY_ROOT HKCU
!define MUI_LANGDLL_REGISTRY_KEY "${PRODUCT_REG_KEY}"
!define MUI_LANGDLL_REGISTRY_VALUENAME "InstallerLanguage"

LicenseLangString UserAgreementData 1033 "${SOURCE_ROOT}\installer\legal\UserAgreement.en.txt"
LicenseLangString UserAgreementData 2052 "${SOURCE_ROOT}\installer\legal\UserAgreement.zh-CN.txt"
LicenseLangString PrivacyPolicyData 1033 "${SOURCE_ROOT}\installer\legal\PrivacyPolicy.en.txt"
LicenseLangString PrivacyPolicyData 2052 "${SOURCE_ROOT}\installer\legal\PrivacyPolicy.zh-CN.txt"

!insertmacro MUI_PAGE_WELCOME
!define MUI_PAGE_HEADER_TEXT "$(UserAgreementTitle)"
!define MUI_PAGE_HEADER_SUBTEXT "$(UserAgreementPrompt)"
!define MUI_LICENSEPAGE_CHECKBOX
!define MUI_LICENSEPAGE_TEXT_TOP "$(UserAgreementPrompt)"
!insertmacro MUI_PAGE_LICENSE $(UserAgreementData)
!define MUI_PAGE_HEADER_TEXT "$(PrivacyPolicyTitle)"
!define MUI_PAGE_HEADER_SUBTEXT "$(PrivacyPolicyPrompt)"
!define MUI_LICENSEPAGE_CHECKBOX
!define MUI_LICENSEPAGE_TEXT_TOP "$(PrivacyPolicyPrompt)"
!insertmacro MUI_PAGE_LICENSE $(PrivacyPolicyData)
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE InstallDirectoryLeave
!insertmacro MUI_PAGE_DIRECTORY
Page custom InstallOptionsCreate InstallOptionsLeave
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
UninstPage custom un.DataOptionsCreate un.DataOptionsLeave
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro GetParameters
!insertmacro GetOptions
!insertmacro un.GetParameters
!insertmacro un.GetOptions

LangString UserAgreementPrompt ${LANG_ENGLISH} "Review the BentoDesk User Agreement. You must accept it to continue."
LangString UserAgreementPrompt ${LANG_SIMPCHINESE} "请阅读 BentoDesk 用户协议。必须同意后才能继续安装。"
LangString UserAgreementTitle ${LANG_ENGLISH} "User Agreement"
LangString UserAgreementTitle ${LANG_SIMPCHINESE} "用户协议"
LangString PrivacyPolicyPrompt ${LANG_ENGLISH} "Review the BentoDesk Privacy Policy. You must accept it to continue."
LangString PrivacyPolicyPrompt ${LANG_SIMPCHINESE} "请阅读 BentoDesk 隐私政策。必须同意后才能继续安装。"
LangString PrivacyPolicyTitle ${LANG_ENGLISH} "Privacy Policy"
LangString PrivacyPolicyTitle ${LANG_SIMPCHINESE} "隐私政策"
LangString WelcomeText ${LANG_ENGLISH} "Native, local-first Windows desktop organization without accounts or telemetry.$\r$\n$\r$\nThe updater uses the default official GitHub Releases API over HTTPS; a controlled BENTODESK_UPDATE_MANIFEST_URL may select an HTTPS, file, or local override. Scheduled checks run immediately and then at the selected cadence (Weekly by default); Settings can select Manual and disable auto-download. Verified setup downloads are only staged, and installation always requires your action.$\r$\n$\r$\nSetup installs BentoDesk for the current user. Administrator privileges are not required."
LangString WelcomeText ${LANG_SIMPCHINESE} "原生、本地优先的 Windows 桌面整理，不需要账号，也不包含遥测。$\r$\n$\r$\n更新器默认通过 HTTPS 使用官方 GitHub Releases API；受控的 BENTODESK_UPDATE_MANIFEST_URL 可选择 HTTPS、file 或本地覆盖源。定时检查启动后立即执行一次，之后按所选周期运行（默认每周）；可在设置中选择 Manual 并关闭自动下载。通过校验的安装包只会暂存，安装始终需要你的明确操作。$\r$\n$\r$\n安装程序将为当前用户安装 BentoDesk，不需要管理员权限。"
LangString FinishText ${LANG_ENGLISH} "BentoDesk is ready. Your local settings will be preserved by default if you uninstall it later."
LangString FinishText ${LANG_SIMPCHINESE} "BentoDesk 已准备就绪。今后卸载时，本地设置默认会被保留。"
LangString RunBentoDesk ${LANG_ENGLISH} "Run BentoDesk"
LangString RunBentoDesk ${LANG_SIMPCHINESE} "运行 BentoDesk"
LangString FinishLinkText ${LANG_ENGLISH} "Open the BentoDesk GitHub page"
LangString FinishLinkText ${LANG_SIMPCHINESE} "打开 BentoDesk GitHub 主页"
LangString OptionsTitle ${LANG_ENGLISH} "Installation options"
LangString OptionsTitle ${LANG_SIMPCHINESE} "安装选项"
LangString OptionsIntro ${LANG_ENGLISH} "Native and local-first; update checks use the default official GitHub HTTPS channel. No telemetry."
LangString OptionsIntro ${LANG_SIMPCHINESE} "原生、本地优先；更新检查默认使用官方 GitHub HTTPS 标准通道，不含遥测。"
LangString AuthorLine ${LANG_ENGLISH} "Author: Fang Han (方寒) · hybridrevis@gmail.com"
LangString AuthorLine ${LANG_SIMPCHINESE} "作者：方寒 · hybridrevis@gmail.com"
LangString CreateDesktopShortcut ${LANG_ENGLISH} "Create a desktop shortcut"
LangString CreateDesktopShortcut ${LANG_SIMPCHINESE} "创建桌面快捷方式"
LangString ExistingInstallRoot ${LANG_ENGLISH} "BentoDesk is already installed in:$\r$\n$1$\r$\n$\r$\nUpgrade and repair must use the existing folder."
LangString ExistingInstallRoot ${LANG_SIMPCHINESE} "BentoDesk 已安装在：$\r$\n$1$\r$\n$\r$\n升级和修复必须继续使用原安装目录。"
LangString CloseAppPrompt ${LANG_ENGLISH} "BentoDesk is still running. Close it and try again. Your layout has not been changed."
LangString CloseAppPrompt ${LANG_SIMPCHINESE} "BentoDesk 仍在运行。请关闭后重试；你的布局没有被更改。"
LangString UninstallDataTitle ${LANG_ENGLISH} "Local settings"
LangString UninstallDataTitle ${LANG_SIMPCHINESE} "本地设置"
LangString UninstallDataIntro ${LANG_ENGLISH} "Your Zone layout and settings are preserved by default. Reinstalling BentoDesk can use them again."
LangString UninstallDataIntro ${LANG_SIMPCHINESE} "默认保留 Zone 布局和设置，重新安装 BentoDesk 后仍可继续使用。"
LangString DeleteUserDataLabel ${LANG_ENGLISH} "Also delete BentoDesk settings, plugins, backups, and local icon cache"
LangString DeleteUserDataLabel ${LANG_SIMPCHINESE} "同时删除 BentoDesk 设置、插件、备份和本地图标缓存"
LangString UserFilesSafe ${LANG_ENGLISH} "This never deletes desktop files or folders represented inside a Zone."
LangString UserFilesSafe ${LANG_SIMPCHINESE} "此操作绝不会删除 Zone 中展示的桌面文件或文件夹。"
LangString PurgeConfirm ${LANG_ENGLISH} "Permanently delete all BentoDesk settings and local cache? Desktop files and folders will not be deleted."
LangString PurgeConfirm ${LANG_SIMPCHINESE} "确定永久删除全部 BentoDesk 设置和本地缓存吗？桌面文件和文件夹不会被删除。"
LangString PurgeFailed ${LANG_ENGLISH} "BentoDesk could not safely delete local state. Uninstall stopped without removing program files."
LangString PurgeFailed ${LANG_SIMPCHINESE} "BentoDesk 无法安全删除本地状态。卸载已停止，程序文件尚未移除。"

Var OptionsDialog
Var DesktopShortcutCheckbox
Var CreateDesktopShortcutState
Var DeleteUserDataCheckbox
Var DeleteUserDataState
Var ValidatedInstallRoot

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP|MB_OK "BentoDesk requires 64-bit Windows."
    Abort
  ${EndIf}
  SetShellVarContext current
  SetRegView 64
  ClearErrors
  ReadRegDWORD $0 HKCU "${PRODUCT_REG_KEY}" "DesktopShortcutOwned"
  IfErrors desktop_shortcut_default
  StrCmp $0 1 desktop_shortcut_checked desktop_shortcut_unchecked
desktop_shortcut_default:
  StrCpy $CreateDesktopShortcutState ${BST_CHECKED}
  Goto desktop_shortcut_option
desktop_shortcut_checked:
  StrCpy $CreateDesktopShortcutState ${BST_CHECKED}
  Goto desktop_shortcut_option
desktop_shortcut_unchecked:
  StrCpy $CreateDesktopShortcutState ${BST_UNCHECKED}
desktop_shortcut_option:
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/NODESKTOPSHORTCUT" $1
  IfErrors desktop_shortcut_option_done
  StrCpy $CreateDesktopShortcutState ${BST_UNCHECKED}
desktop_shortcut_option_done:
  IfSilent silent_acceptance interactive_language

silent_acceptance:
  ReadRegStr $2 HKCU "${PRODUCT_REG_KEY}" "AcceptedAgreementRevision"
  StrCmp $2 "${LEGAL_REVISION}" privacy_acceptance
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/ACCEPTAGREEMENT" $1
  IfErrors silent_rejected privacy_acceptance

privacy_acceptance:
  ReadRegStr $2 HKCU "${PRODUCT_REG_KEY}" "AcceptedPrivacyRevision"
  StrCmp $2 "${LEGAL_REVISION}" init_done
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/ACCEPTPRIVACY" $1
  IfErrors silent_rejected init_done

silent_rejected:
  SetErrorLevel 2
  Quit

interactive_language:
  !insertmacro MUI_LANGDLL_DISPLAY
init_done:
FunctionEnd

Function InstallRootMatchesRegistry
  StrCpy $0 1
  ReadRegStr $1 HKCU "${PRODUCT_REG_KEY}" "InstallDir"
  StrCmp $1 "" install_root_check_done
  ClearErrors
  GetFullPathName $2 "$INSTDIR"
  IfErrors install_root_mismatch
  ClearErrors
  GetFullPathName $3 "$1"
  IfErrors install_root_mismatch
  StrCmp $2 $3 install_root_check_done install_root_mismatch
install_root_mismatch:
  StrCpy $0 0
install_root_check_done:
  Push $0
FunctionEnd

Function InstallDirectoryLeave
  Call InstallRootMatchesRegistry
  Pop $0
  StrCmp $0 1 install_directory_done
  ReadRegStr $1 HKCU "${PRODUCT_REG_KEY}" "InstallDir"
  MessageBox MB_ICONEXCLAMATION|MB_OK "$(ExistingInstallRoot)"
  Abort
install_directory_done:
FunctionEnd

Function un.onInit
  SetShellVarContext current
  SetRegView 64
  StrCpy $DeleteUserDataState ${BST_UNCHECKED}
  ReadRegStr $2 HKCU "${PRODUCT_REG_KEY}" "InstallDir"
  ${If} $2 == ""
    Goto un_reject_install_root
  ${EndIf}
  ${If} $INSTDIR == ""
    Goto un_reject_install_root
  ${EndIf}
  ClearErrors
  GetFullPathName $3 "$INSTDIR"
  IfErrors un_reject_install_root
  ClearErrors
  GetFullPathName $4 "$2"
  IfErrors un_reject_install_root
  StrCmp $3 $4 un_install_root_verified un_reject_install_root
un_reject_install_root:
  SetErrorLevel 4
  Quit
un_install_root_verified:
  StrCpy $ValidatedInstallRoot $3
  !insertmacro MUI_UNGETLANGUAGE
  ${un.GetParameters} $0
  ClearErrors
  ${un.GetOptions} $0 "/PURGEUSERDATA" $1
  IfErrors un_init_done
  ClearErrors
  ${un.GetOptions} $0 "/CONFIRMPURGE=" $1
  IfErrors un_reject_silent_purge
  StrCmp $1 "${PURGE_CONFIRM_TOKEN}" un_purge_enabled un_reject_silent_purge
un_reject_silent_purge:
  SetErrorLevel 3
  Quit
un_purge_enabled:
  StrCpy $DeleteUserDataState ${BST_CHECKED}
un_init_done:
FunctionEnd

!macro DEFINE_CLOSE_FUNCTION Prefix
Function ${Prefix}CloseRunningBentoDesk
  FindWindow $0 "BentoDeskShell" ""
  ${If} $0 == 0
    Return
  ${EndIf}
  SendMessage $0 ${WM_HOTKEY} 16973 0 /TIMEOUT=1000
  StrCpy $1 0
  ${Do}
    FindWindow $0 "BentoDeskShell" ""
    ${If} $0 == 0
      Return
    ${EndIf}
    ${If} $1 >= 40
      MessageBox MB_ICONEXCLAMATION|MB_OK "$(CloseAppPrompt)"
      Abort
    ${EndIf}
    Sleep 250
    IntOp $1 $1 + 1
  ${Loop}
FunctionEnd
!macroend

!insertmacro DEFINE_CLOSE_FUNCTION ""
!insertmacro DEFINE_CLOSE_FUNCTION "un."

Function InstallOptionsCreate
  !insertmacro MUI_HEADER_TEXT "$(OptionsTitle)" "$(OptionsIntro)"
  nsDialogs::Create 1018
  Pop $OptionsDialog
  ${If} $OptionsDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 14u "$(AuthorLine)"
  Pop $0

  ${NSD_CreateLink} 0 28u 48% 12u "github.com/ZRainbow1275"
  Pop $0
  ${NSD_OnClick} $0 OpenGitHub
  ${NSD_CreateLink} 52% 28u 48% 12u "x.com/zrainbo · @zrainbo"
  Pop $0
  ${NSD_OnClick} $0 OpenX
  ${NSD_CreateLink} 0 48u 100% 12u "hybridrevis@gmail.com"
  Pop $0
  ${NSD_OnClick} $0 OpenEmail

  ${NSD_CreateCheckBox} 0 -18u 100% 12u "$(CreateDesktopShortcut)"
  Pop $DesktopShortcutCheckbox
  ${NSD_SetState} $DesktopShortcutCheckbox $CreateDesktopShortcutState
  nsDialogs::Show
FunctionEnd

Function InstallOptionsLeave
  ${NSD_GetState} $DesktopShortcutCheckbox $CreateDesktopShortcutState
FunctionEnd

Function OpenGitHub
  Pop $0
  ExecShell "open" "${PRODUCT_WEB_SITE}"
FunctionEnd

Function OpenX
  Pop $0
  ExecShell "open" "${PRODUCT_X_SITE}"
FunctionEnd

Function OpenEmail
  Pop $0
  ExecShell "open" "mailto:${PRODUCT_EMAIL}"
FunctionEnd

Section "BentoDesk" SEC_MAIN
  Call InstallRootMatchesRegistry
  Pop $0
  StrCmp $0 1 install_root_verified
  SetErrorLevel 5
  IfSilent install_root_quit install_root_message
install_root_message:
  ReadRegStr $1 HKCU "${PRODUCT_REG_KEY}" "InstallDir"
  MessageBox MB_ICONSTOP|MB_OK "$(ExistingInstallRoot)"
  Abort
install_root_quit:
  Quit
install_root_verified:
  Call CloseRunningBentoDesk
  SetOutPath "$INSTDIR"
  File /oname=BentoDesk.exe "${APP_EXE}"
  File /oname=LICENSE "${SOURCE_ROOT}\LICENSE"

  SetOutPath "$INSTDIR\legal"
  File /oname=UserAgreement.en.txt "${SOURCE_ROOT}\installer\legal\UserAgreement.en.txt"
  File /oname=UserAgreement.zh-CN.txt "${SOURCE_ROOT}\installer\legal\UserAgreement.zh-CN.txt"
  File /oname=PrivacyPolicy.en.txt "${SOURCE_ROOT}\installer\legal\PrivacyPolicy.en.txt"
  File /oname=PrivacyPolicy.zh-CN.txt "${SOURCE_ROOT}\installer\legal\PrivacyPolicy.zh-CN.txt"
  File /oname=Remove-BentoDeskData.ps1 "${SOURCE_ROOT}\installer\Remove-BentoDeskData.ps1"

  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  WriteRegStr HKCU "${PRODUCT_REG_KEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${PRODUCT_REG_KEY}" "InstallerLanguage" $LANGUAGE
  WriteRegStr HKCU "${PRODUCT_REG_KEY}" "AcceptedAgreementRevision" "${LEGAL_REVISION}"
  WriteRegStr HKCU "${PRODUCT_REG_KEY}" "AcceptedPrivacyRevision" "${LEGAL_REVISION}"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "DisplayName" "BentoDesk"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "DisplayIcon" "$INSTDIR\BentoDesk.exe"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "URLInfoAbout" "${PRODUCT_WEB_SITE}"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "HelpLink" "${PRODUCT_WEB_SITE}/issues"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "Contact" "${PRODUCT_EMAIL}"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${PRODUCT_UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${PRODUCT_UNINST_KEY}" "NoRepair" 1

  CreateDirectory "$SMPROGRAMS\BentoDesk"
  CreateShortcut "$SMPROGRAMS\BentoDesk\BentoDesk.lnk" "$INSTDIR\BentoDesk.exe" "" "$INSTDIR\BentoDesk.exe" 0
  CreateShortcut "$SMPROGRAMS\BentoDesk\Uninstall BentoDesk.lnk" "$INSTDIR\Uninstall.exe"
  ClearErrors
  ReadRegDWORD $7 HKCU "${PRODUCT_REG_KEY}" "DesktopShortcutOwned"
  IfErrors desktop_shortcut_was_unowned
  Goto desktop_shortcut_previous_read
desktop_shortcut_was_unowned:
  StrCpy $7 0
desktop_shortcut_previous_read:
  ${If} $CreateDesktopShortcutState == ${BST_CHECKED}
    StrCmp $7 1 desktop_shortcut_create
    IfFileExists "$DESKTOP\BentoDesk.lnk" desktop_shortcut_preserve_unowned
desktop_shortcut_create:
    ClearErrors
    CreateShortcut "$DESKTOP\BentoDesk.lnk" "$INSTDIR\BentoDesk.exe" "" "$INSTDIR\BentoDesk.exe" 0
    IfErrors desktop_shortcut_preserve_unowned
    WriteRegDWORD HKCU "${PRODUCT_REG_KEY}" "DesktopShortcutOwned" 1
    Goto desktop_shortcut_done
desktop_shortcut_preserve_unowned:
    WriteRegDWORD HKCU "${PRODUCT_REG_KEY}" "DesktopShortcutOwned" 0
  ${Else}
    StrCmp $7 1 0 desktop_shortcut_not_owned
    Delete "$DESKTOP\BentoDesk.lnk"
desktop_shortcut_not_owned:
    WriteRegDWORD HKCU "${PRODUCT_REG_KEY}" "DesktopShortcutOwned" 0
  ${EndIf}
desktop_shortcut_done:
SectionEnd

Function un.DataOptionsCreate
  !insertmacro MUI_HEADER_TEXT "$(UninstallDataTitle)" "$(UninstallDataIntro)"
  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}

  ${NSD_CreateCheckBox} 0 8u 100% 24u "$(DeleteUserDataLabel)"
  Pop $DeleteUserDataCheckbox
  ${NSD_SetState} $DeleteUserDataCheckbox $DeleteUserDataState
  ${NSD_CreateLabel} 0 48u 100% 30u "$(UserFilesSafe)"
  Pop $0
  nsDialogs::Show
FunctionEnd

Function un.DataOptionsLeave
  ${NSD_GetState} $DeleteUserDataCheckbox $DeleteUserDataState
  ${If} $DeleteUserDataState == ${BST_CHECKED}
    MessageBox MB_ICONEXCLAMATION|MB_YESNO|MB_DEFBUTTON2 "$(PurgeConfirm)" IDYES purge_confirmed
    Abort
purge_confirmed:
  ${EndIf}
FunctionEnd

Section "Uninstall"
  Call un.CloseRunningBentoDesk

  ${If} $DeleteUserDataState == ${BST_CHECKED}
    nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$ValidatedInstallRoot\legal\Remove-BentoDeskData.ps1" -RoamingRoot "$APPDATA\BentoDesk" -RoamingParent "$APPDATA" -PortableRoot "$ValidatedInstallRoot\BentoDeskData" -PortableParent "$ValidatedInstallRoot" -PortableMarker "$ValidatedInstallRoot\.bentodesk-portable"'
    Pop $0
    ${If} $0 != 0
      MessageBox MB_ICONSTOP|MB_OK "$(PurgeFailed)"
      Abort
    ${EndIf}
  ${EndIf}

  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "BentoDesk"
  ClearErrors
  ReadRegDWORD $0 HKCU "${PRODUCT_REG_KEY}" "DesktopShortcutOwned"
  IfErrors un_desktop_shortcut_done
  StrCmp $0 1 0 un_desktop_shortcut_done
  Delete "$DESKTOP\BentoDesk.lnk"
un_desktop_shortcut_done:
  Delete "$SMPROGRAMS\BentoDesk\BentoDesk.lnk"
  Delete "$SMPROGRAMS\BentoDesk\Uninstall BentoDesk.lnk"
  RMDir "$SMPROGRAMS\BentoDesk"

  Delete "$INSTDIR\BentoDesk.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\legal\UserAgreement.en.txt"
  Delete "$INSTDIR\legal\UserAgreement.zh-CN.txt"
  Delete "$INSTDIR\legal\PrivacyPolicy.en.txt"
  Delete "$INSTDIR\legal\PrivacyPolicy.zh-CN.txt"
  Delete "$INSTDIR\legal\Remove-BentoDeskData.ps1"
  RMDir "$INSTDIR\legal"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "${PRODUCT_UNINST_KEY}"
  DeleteRegKey HKCU "${PRODUCT_REG_KEY}"
SectionEnd
