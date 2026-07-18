# SessionGate Fullscreen Remote Desktop Test Report

Date: 2026-07-18

Environment: Microsoft Edge 150, Caddy HTTPS, Guacamole 1.6.0, guacd 1.6.0,
and the Hyper-V Windows Server 2025 VM at `172.31.98.16`.

## Automated results

| Check | Result |
|---|---|
| Portal workspace served over HTTPS | Pass |
| Strict portal CSP retained | Pass |
| Guacamole frame restricted to same origin | Pass |
| RDP iframe loaded from `/guacamole/` | Pass |
| Browser joined the guacd connection | Pass |
| RDP security mode | Pass: NLA |
| RDP certificate validation | Pass |
| Edge Keyboard Lock API available | Pass |
| Fullscreen button and state handler present | Pass |
| Programmatic/background fullscreen entry | Not applicable: denied by Edge user-activation policy |

The final fullscreen transition requires a physical click on **Full screen**.
After the first click, Edge may display a Keyboard Lock permission prompt. The
operator must allow it and click **Full screen** again. The button must then read
**Exit full screen** and the desktop must fill the display.

Operating-system secure shortcuts remain outside browser control. Use
Guacamole's on-screen keyboard controls for `Ctrl+Alt+Delete` and other Windows
secure-attention sequences.
