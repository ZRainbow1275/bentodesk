; BentoDesk NSIS Installer Hooks
; Custom uninstall logic to fully clean up BentoDesk data.

!macro NSIS_HOOK_PREINSTALL
  ; No pre-install hooks needed
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Create "Uninstall BentoDesk" shortcut in Start Menu
  CreateDirectory "$SMPROGRAMS\BentoDesk"
  CreateShortCut "$SMPROGRAMS\BentoDesk\卸载 BentoDesk.lnk" "$INSTDIR\uninstall.exe"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; --- Pre-uninstall: Clean up BentoDesk-specific data ---

  ; 1. Delete the Windows Task Scheduler task (silent, ignore errors)
  nsExec::ExecToLog 'schtasks.exe /delete /tn "BentoDesk" /f'

  ; 2. Restore hidden files from .bentodesk/ back to Desktop
  ;    The main app normally does this on exit, but if the user force-killed it,
  ;    files may still be hidden. We attempt to move them back.
  ;    Note: This is a best-effort operation using a simple directory scan.

  ; Get the user's Desktop path
  StrCpy $0 "$DESKTOP"

  ; Check if .bentodesk directory exists on Desktop
  IfFileExists "$0\.bentodesk\*.*" 0 skip_restore
    ; Remove hidden/system attributes from .bentodesk folder and ALL contents recursively
    nsExec::ExecToLog 'attrib -h -s "$0\.bentodesk"'
    nsExec::ExecToLog 'attrib -h -s /s /d "$0\.bentodesk\*.*"'

    ; Show a message about remaining files
    MessageBox MB_OK|MB_ICONINFORMATION "BentoDesk 的隐藏文件夹 .bentodesk 可能包含您之前整理的文件。$\n$\n卸载程序已取消其隐藏属性，请检查桌面上的 .bentodesk 文件夹并手动恢复文件。" /SD IDOK
  skip_restore:

  ; 3. Clean up AppData directory
  RMDir /r "$LOCALAPPDATA\com.bentodesk.app"

  ; 4. Clean up any safe_mode.json or guardian.log in install dir
  Delete "$INSTDIR\guardian.log"
  Delete "$INSTDIR\safe_mode.json"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Remove Start Menu shortcuts
  Delete "$SMPROGRAMS\BentoDesk\卸载 BentoDesk.lnk"
  RMDir "$SMPROGRAMS\BentoDesk"
!macroend
