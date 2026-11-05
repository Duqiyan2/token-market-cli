; This marker belongs only to the reviewed current-user NSIS lifecycle contract.
; The release workflow pins Tauri CLI 2.12.4 / bundler 2.10.4.
!if "${INSTALLMODE}" != "currentUser"
  !error "Token Market automatic updates require NSIS currentUser"
!endif
!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "${UNINSTKEY}" "TokenMarketUpdatePolicy" "nsis-currentUser-v1-bundler2.10.4"
  WriteRegStr HKCU "${UNINSTKEY}" "TokenMarketUpdateDirectory" "$INSTDIR"
!macroend
