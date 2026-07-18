# SessionGate Test Report — 2026-07-18

> Historical pre-fix result. The NLA and certificate failures documented below
> were fixed later the same day. See `TEST-REPORT-POST-FIX-2026-07-18.md`.

## Result

**Overall: FAIL — not end-to-end functional.**

The Hyper-V Windows VM, RDP listener, portal API, PostgreSQL control plane,
Guacamole authentication, browser tunnel, and default-deny policy plumbing all
worked. The interactive desktop did not work: guacd/FreeRDP selected NLA but
failed security negotiation before Windows recorded authentication event 1149.

Clipboard, file transfer, printing, audio, microphone, recording, desktop
latency, and concurrent desktop performance cannot be marked as tested until a
desktop session authenticates successfully.

## Environment

Test time: 2026-07-18 14:28 KST.

| Component | Configuration |
|---|---|
| Host | Windows, 32 logical processors, 95.6 GiB RAM |
| Hypervisor | Hyper-V |
| VM | `vpn-rdp-windows-test`, generation 2 |
| Guest | Windows Server 2025 Standard Evaluation, Desktop Experience |
| Guest address | `172.31.98.16` |
| VM resources | 4 vCPU, 4 GiB assigned during measurement |
| VM disk | 40 GiB logical, 16.32 GiB allocated |
| Portal | Rust/Axum container, test port 18080 |
| Guacamole | 1.6.0, test port 18081 |
| guacd | 1.6.0 |
| PostgreSQL | 17-bookworm |
| Browser | Microsoft Edge, isolated headless profile |

The VM used NLA and a pinned SHA-256 RDP certificate. Test credentials and lab
secrets were temporary and are intentionally omitted from this report.

## Verification matrix

| Test | Result | Evidence |
|---|---:|---|
| Rust formatting | PASS | `cargo fmt --all -- --check` exited zero |
| Rust unit tests | PASS | 4 passed, 0 failed |
| Clippy | PASS | All targets with warnings denied |
| Compose rendering | PASS | `docker compose config --quiet` exited zero |
| VM state | PASS | Running, Hyper-V status normal |
| VM heartbeat | PASS | `OK` |
| Windows RDP service | PASS | `TermService` running |
| Windows RDP firewall | PASS | Enabled rules present |
| RDP TCP reachability | PASS | Host reached `172.31.98.16:3389` |
| NLA policy | PASS | `UserAuthentication=1` |
| Portal health | PASS | HTTP 200, including database check |
| Guacamole HTTP | PASS | HTTP 200 |
| Anonymous API denial | PASS | HTTP 401 |
| Missing-origin denial | PASS | HTTP 403 |
| Unknown-target default denial | PASS | HTTP 403 |
| Default clipboard denial | PASS | Effective policy reported blocked |
| Session persistence | PASS | Two launch sessions persisted |
| Audit persistence | PASS | Two allowed and one denied event |
| Encrypted Guacamole authentication | PASS | JSON data source accepted assertion |
| Browser tunnel creation | PASS | guacd created a client and browser user joined |
| guacd NLA selection | PASS | `Security mode: NLA` |
| Windows listener contact | PASS | Windows event 261 recorded |
| Windows authentication | **FAIL** | Zero event 1149 records |
| Interactive desktop | **FAIL** | guacd closed during security negotiation |
| Redirect-policy behavior in desktop | BLOCKED | No authenticated desktop |
| Desktop performance | BLOCKED | No authenticated desktop |

## API latency

The authenticated target-inventory endpoint was measured sequentially with a
reused HTTP client from the host. This is a local control-plane latency test,
not a load or concurrency benchmark.

| Metric | Result |
|---|---:|
| Samples | 200 |
| Minimum | 1.16 ms |
| Mean | 1.76 ms |
| p50 | 1.43 ms |
| p95 | 1.91 ms |
| p99 | 4.95 ms |
| Maximum | 35.41 ms |

## Browser and container measurements

The browser opened a fresh 30-second portal launch assertion. Resource values
were sampled during the attempted RDP negotiation.

| Process/container | CPU | Memory |
|---|---:|---:|
| Edge processes | 4.22 CPU-seconds accumulated | 484.8 MiB aggregate working set |
| Portal | 0.00% | 2.863 MiB |
| Guacamole | 1.64% | 318.2 MiB |
| guacd | 21.75% | 16.75 MiB |
| PostgreSQL | 0.00% | 33.9 MiB |

Edge memory is aggregate process working set and should be treated as an
approximation. The guacd sample covers failed negotiation, not desktop render
load. It must not be used as an interactive-session capacity number.

## Browser/VM failure evidence

The correlated sequence was:

1. Portal created and persisted a launch session.
2. Edge redeemed the encrypted assertion through Guacamole.
3. Guacamole created an RDP client.
4. The browser user joined the guacd connection.
5. guacd logged `Security mode: NLA`.
6. Windows logged listener event 261.
7. Windows logged no authentication event 1149.
8. guacd logged `Security negotiation failed` and removed the connection.

This places the failure after TCP listener contact but before Windows user
authentication. It does not support a claim that credentials were rejected.

## Container image evidence

| Image | Local image ID | Size |
|---|---|---:|
| `vpn-solution-portal` | `sha256:12ae4e8e87039fd7b5ca8d492a8a38a5818f71658f61bf1cdaf0ae37e9295a1c` | 30.2 MiB |
| `guacamole/guacamole:1.6.0` | `sha256:f344085e618bb05e22b964b0208dbd06d3468275bac70206f93805245e067b40` | 394.3 MiB |
| `guacamole/guacd:1.6.0` | `sha256:8974eaa9ba32f713daf311e7cc8cd7e4cdfba1edea39eed75524e78ef4b08f4f` | 123.0 MiB |
| `postgres:17-bookworm` | `sha256:4f736ae292687621d4dbe0d499ffd024a36bd2ee7d8ca6f2ccd4c800f047b394` | 148.9 MiB |

These are local image IDs, not registry digest pins.

## Required follow-up

1. Run guacd with FreeRDP debug logging and capture the exact CredSSP/TLS error.
2. Verify Windows Server 2025 compatibility with the FreeRDP build embedded in
   `guacamole/guacd:1.6.0`.
3. Implement a credential-broker or vault flow; the production portal currently
   does not provide RDP credentials in its assertion.
4. Retest with NLA and certificate validation still enabled.
5. Only after authentication succeeds, execute clipboard direction, upload,
   download, printing, audio, microphone, recording, expiry, reconnect,
   concurrent-session, and first-frame latency tests.

No production-readiness or “all working” claim should be made from this run.
