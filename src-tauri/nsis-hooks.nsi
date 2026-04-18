; BentoDesk NSIS Installer Hooks
; Custom uninstall logic to fully clean up BentoDesk data.

!include "FileFunc.nsh"
!insertmacro GetOptions

!macro NSIS_HOOK_PREINSTALL
  ; No pre-install hooks needed
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Create "Uninstall BentoDesk" shortcut in Start Menu
  CreateDirectory "$SMPROGRAMS\BentoDesk"
  CreateShortCut "$SMPROGRAMS\BentoDesk\卸载 BentoDesk.lnk" "$INSTDIR\uninstall.exe"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Tauri v2 upgrade flow passes /UPDATE to the old uninstaller (installer.nsi:331).
  ; We must skip all destructive cleanup in that case, or upgrading from v1.1 → v1.2
  ; would delete user settings/timeline/icons and expose the hidden .bentodesk folder
  ; on the Desktop.
  ${GetOptions} $CMDLINE "/UPDATE" $0
  ${IfNot} ${Errors}
    Goto skip_full_cleanup
  ${EndIf}

  ; --- Real uninstall path (user explicitly removing the app) ---

  ; 1. Delete the Windows Task Scheduler task (silent, ignore errors)
  nsExec::ExecToLog 'schtasks.exe /delete /tn "BentoDesk" /f'

  ; 2. Restore hidden files from .bentodesk/ back to Desktop
  ;    The main app normally does this on exit, but if the user force-killed it,
  ;    files may still be hidden. We attempt to move them back.
  StrCpy $0 "$DESKTOP"

  ; Check if .bentodesk directory exists on Desktop
  IfFileExists "$0\.bentodesk\*.*" 0 skip_restore
    ; Remove hidden/system attributes from .bentodesk folder and ALL contents recursively
    nsExec::ExecToLog 'attrib -h -s "$0\.bentodesk"'
    nsExec::ExecToLog 'attrib -h -s /s /d "$0\.bentodesk\*.*"'

    MessageBox MB_OK|MB_ICONINFORMATION "BentoDesk 的隐藏文件夹 .bentodesk 可能包含您之前整理的文件。$\n$\n卸载程序已取消其隐藏属性，请检查桌面上的 .bentodesk 文件夹并手动恢复文件。" /SD IDOK
  skip_restore:

  ; 3. Clean up install-dir debug artifacts (these are NOT user data)
  Delete "$INSTDIR\guardian.log"
  Delete "$INSTDIR\safe_mode.json"

  ; NOTE: We deliberately DO NOT RMDir $LOCALAPPDATA\com.bentodesk.app here.
  ; Tauri's default uninstaller (installer.nsi ~L819) handles that under the
  ; user's "Delete app data" checkbox + $UpdateMode guard — respecting user choice
  ; and never deleting during upgrade. Doing it ourselves bypasses both protections.

  skip_full_cleanup:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Skip Start Menu cleanup during upgrade — POSTINSTALL will recreate it anyway,
  ; but removing-then-recreating causes a visible flicker in Explorer.
  ${GetOptions} $CMDLINE "/UPDATE" $0
  ${If} ${Errors}
    Delete "$SMPROGRAMS\BentoDesk\卸载 BentoDesk.lnk"
    RMDir "$SMPROGRAMS\BentoDesk"
  ${EndIf}
!macroend
