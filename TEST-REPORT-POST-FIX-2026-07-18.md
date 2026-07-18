# SessionGate Post-Fix Test Report — 2026-07-18

## Result

**Interactive desktop authentication: PASS.**

The two failures from the initial report were fixed without disabling NLA or
certificate validation:

1. guacd now uses `/tmp` as its ephemeral home, allowing FreeRDP to initialize
   its certificate store while the container root remains read-only.
2. RDP SHA-256 pins are emitted in FreeRDP's required uppercase,
   colon-separated format.
3. The lab portal accepts temporary Windows credentials and includes them only
   in the encrypted 30-second launch assertion.

## Evidence

| Test | Result | Evidence |
|---|---:|---|
| Unit tests | PASS | 5 passed, including explicit credential inclusion |
| Clippy | PASS | All targets, warnings denied |
| Read-only guacd root | PASS | Retained |
| Ephemeral FreeRDP home | PASS | `HOME=/tmp`, backed by tmpfs |
| NLA | PASS | guacd logged `Security mode: NLA` |
| Certificate pin | PASS | No certificate-validation failure |
| Windows authentication | PASS | Event 1149 recorded |
| RDP device login | PASS | guacd logged `RDPDR user logged on` |
| Windows session | PASS | `Administrator` active on `rdp-tcp#0` |
| Negotiation | PASS | No security-negotiation failure |
| Recording disabled | PASS | Zero recording files created |
| Credential persistence | PASS | Session table contains policy only; credentials are not schema fields |

The authenticated VM remained Windows Server 2025 Standard Evaluation at
`172.31.98.16`, with NLA enabled and its SHA-256 certificate pinned.

## Root causes

Before the fix, FreeRDP logged:

```text
error creating directory '/home/guacd/.config/freerdp'
certificate store initialization failed
ERRCONNECT_SECURITY_NEGO_CONNECT_FAILED
```

After moving the ephemeral home to `/tmp`, that failure disappeared. The next
failure was strict certificate validation because the unseparated hexadecimal
fingerprint was not valid xfreerdp fingerprint syntax. Normalizing the pin to
`sha256:AA:BB:...` fixed strict validation.

## Remaining qualification work

This report does not convert the lab into a production-ready service. The
following are still required:

- Explicit in-desktop clipboard direction tests
- Upload and download tests with enabled and disabled policies
- Printing, audio, and microphone behavioral tests
- Recording-enabled storage and playback tests
- Enforced maximum session duration and termination state updates
- Reconnect, concurrent-user, and sustained desktop performance tests
- Production HTTPS, MFA/JWT, administrator authorization, credential vaulting,
  and one-time launch assertions

The original authentication and interactive-desktop failures are fixed. The
remaining items are unimplemented or unqualified capabilities, not evidence
from the original NLA failure.
