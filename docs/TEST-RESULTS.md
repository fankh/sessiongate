# SessionGate Test Results

Last consolidated qualification: 2026-07-18.

## Environment

- Microsoft Edge against the Caddy HTTPS endpoint
- Docker Compose with portal, PostgreSQL, Guacamole, guacd, and Caddy
- Generation 2 Windows Hyper-V VM with NLA-enabled RDP
- Certificate fingerprint pinning and server-enforced default-deny policy

## Passed

- all portal pages and assets returned HTTP 200 with expected security headers;
- desktop and 390-pixel mobile layouts had no horizontal overflow or runtime errors;
- built-in authentication accepted accounts without OTP only when no TOTP secret
  was configured;
- target listing and launch authorization respected persisted assignments;
- clipboard directions and other redirection controls remained independently
  default-deny;
- invalid target, malformed assertion, expired assertion, NLA failure, incorrect
  Windows credentials, and certificate mismatch failed closed;
- valid Windows credentials, NLA, and the approved certificate pin reached the
  real remote desktop through Guacamole and guacd;
- full-screen activation and remote workspace keyboard handling worked in Edge;
- session and audit rows were persisted in PostgreSQL;
- Rust tests passed 13 of 13 and Clippy completed with zero warnings.

## Historical fixes incorporated

The first VM run exposed an NLA negotiation mismatch and an incorrect
certificate fingerprint. The integration configuration was corrected, the
actual target certificate was pinned, and the post-fix real desktop run passed.
These failures remain covered by fail-closed test expectations.

## Remaining qualification

Production release still requires concurrency/load limits, enforceable
server-side termination, external credential-provider qualification, SIEM
delivery operation, restore testing, and public-host TLS qualification. These
items are tracked in [../ROADMAP.md](../ROADMAP.md).

For reproduction, follow [TESTING.md](TESTING.md) and use the scripts under
`scripts/`.
