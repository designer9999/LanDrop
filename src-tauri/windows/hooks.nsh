; Add Windows Firewall rule after installation so LanDrop can accept
; incoming LAN connections without the user having to manually allow it.
!macro NSIS_HOOK_POSTINSTALL
  ; Add inbound TCP rule for LanDrop
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="LanDrop TCP" dir=in action=allow program="$INSTDIR\landrop.exe" protocol=TCP localport=29171 profile=private enable=yes'
  ; Add inbound UDP rule for mDNS discovery
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="LanDrop mDNS" dir=in action=allow program="$INSTDIR\landrop.exe" protocol=UDP localport=5353 profile=private enable=yes'
!macroend

; Remove firewall rules on uninstall
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="LanDrop TCP" program="$INSTDIR\landrop.exe"'
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="LanDrop mDNS" program="$INSTDIR\landrop.exe"'
  ; Clean up the broad legacy rule used by releases before the rules were scoped.
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="LanDrop" program="$INSTDIR\landrop.exe"'
!macroend
