; NSIS installer hooks (see tauri.conf.json > bundle.windows.nsis.installerHooks).
;
; Vuoom streams recordings to raw frame stores under %TEMP%\vuoom-recovery so a crash
; mid-take is recoverable. Those stores are gigabyte-scale, so uninstalling must not leave
; them behind. $TEMP resolves to the same per-user temp dir that std::env::temp_dir() uses,
; so this clears exactly the folder the app writes. RMDir /r is a no-op if it's absent.
!macro NSIS_HOOK_POSTUNINSTALL
  RMDir /r "$TEMP\vuoom-recovery"
!macroend
