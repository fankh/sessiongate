# SessionGate container deployment

This guide deploys SessionGate with Docker Compose. The
stack contains Caddy, the Rust portal, PostgreSQL, Apache Guacamole, and
`guacd`.

The supplied configuration is safe for local evaluation: every published port
binds to `127.0.0.1`, Caddy issues an internal certificate for `localhost`, and
the database and control networks are not exposed. Do not expose the direct
portal (`18080`) or Guacamole (`18081`) ports on a server.

## Prerequisites

- Docker Engine 24+ with the Compose v2 plugin, or current Docker Desktop
- Git
- An RDP destination reachable from the Docker host
- The SHA-256 fingerprint of the destination's RDP certificate
- At least 2 CPU cores, 4 GB RAM, and 10 GB free disk for a small deployment

Confirm Docker is ready:

```sh
docker version
docker compose version
```

## 1. Configure the deployment

Clone the repository, enter this directory, and create the untracked
environment file:

```sh
git clone https://github.com/fankh/sessiongate.git
cd sessiongate
cp .env.example .env
```

In PowerShell, use `Copy-Item .env.example .env` instead of `cp`.

Generate independent secrets. On Linux or macOS:

```sh
openssl rand -hex 32
openssl rand -hex 16
openssl rand -base64 36
```

In PowerShell:

```powershell
[Convert]::ToHexString([Security.Cryptography.RandomNumberGenerator]::GetBytes(32)).ToLower()
[Convert]::ToHexString([Security.Cryptography.RandomNumberGenerator]::GetBytes(16)).ToLower()
[Convert]::ToBase64String([Security.Cryptography.RandomNumberGenerator]::GetBytes(36))
```

Edit `.env` and set every placeholder:

```dotenv
PORTAL_BEARER_TOKEN=<first generated value; at least 32 characters>
PORTAL_USER=lab-user
PORTAL_ALLOWED_ORIGIN=https://localhost:18443
GUACAMOLE_JSON_SECRET_KEY=<second generated value; exactly 32 hexadecimal digits>
POSTGRES_DB=vpn
POSTGRES_USER=vpn
POSTGRES_PASSWORD=<third generated value>
RDP_TARGET_HOST=10.20.30.40
RDP_CERTIFICATE_SHA256=<64-character lowercase SHA-256 fingerprint>
RDP_DOMAIN=

# Optional initial management user. Remove these values after first startup.
PORTAL_BOOTSTRAP_USERNAME=admin
PORTAL_BOOTSTRAP_PASSWORD=<a separate strong password>
# Leave empty to permit login without OTP until MFA is enrolled.
PORTAL_BOOTSTRAP_TOTP_HEX=
```

`GUACAMOLE_JSON_SECRET_KEY` must be exactly 32 hexadecimal characters for the
Guacamole JSON authentication extension. Do not reuse it as the database,
user, or bearer-token secret.

The `.env` file is ignored by Git. Restrict it to the deployment administrator
and store an encrypted backup outside the host.

## 2. Validate and start

Resolve the Compose model before creating containers:

```sh
docker compose config --quiet
docker compose build --pull
docker compose up -d
docker compose ps
```

On its first start, the portal waits for PostgreSQL, applies all migrations,
creates the optional bootstrap administrator, and creates the configured lab
target and assignment. Watch startup without exposing secrets:

```sh
docker compose logs -f --tail=100 portal caddy guacamole guacd database
```

Wait until PostgreSQL reports healthy and the portal stays running. Verify the
portal through Caddy:

```sh
curl -kfsS https://localhost:18443/healthz
curl -kI https://localhost:18443/login.html
```

PowerShell equivalent:

```powershell
curl.exe -kfsS https://localhost:18443/healthz
curl.exe -kI https://localhost:18443/login.html
```

Open <https://localhost:18443/login.html>. The local Caddy authority is not
trusted by the operating system by default, so a browser warning is expected
for this local-only profile. Sign in with the bootstrap account, then select
an assigned desktop. If no bootstrap account was configured, the lab bearer
token can load the target on the **My desktops** page.

After the administrator has been created successfully, remove
`PORTAL_BOOTSTRAP_PASSWORD` and `PORTAL_BOOTSTRAP_TOTP_HEX` from `.env`, then
recreate the portal container:

```sh
docker compose up -d --force-recreate portal
```

## 3. Verify the running stack

```sh
docker compose ps
docker compose exec database pg_isready -U vpn -d vpn
curl -kfsS https://localhost:18443/healthz
```

Expected published listeners are loopback only:

| URL | Purpose |
|---|---|
| `https://127.0.0.1:18443` | Supported browser entry point through Caddy |
| `http://127.0.0.1:18080` | Direct portal diagnostics; never expose publicly |
| `http://127.0.0.1:18081` | Direct Guacamole diagnostics; never expose publicly |

The RDP host must allow TCP 3389 from the Docker/`guacd` network path. A launch
will fail closed if the configured certificate fingerprint does not match.

## Hosted server configuration

The checked-in `Caddyfile` is intentionally limited to `localhost`. For a
server deployment:

1. Create a DNS record such as `rdp.example.com` pointing to the server.
2. Permit inbound TCP 443 to the server and keep 18080, 18081, 5432, and 4822
   blocked.
3. Change the Caddy site address from `https://localhost:8443` to the public
   hostname and remove `tls internal` so Caddy obtains a publicly trusted
   certificate.
4. Publish Caddy as `443:8443` (or map it through the platform load balancer).
5. Set `PORTAL_ALLOWED_ORIGIN=https://rdp.example.com` and
   `GUACAMOLE_PUBLIC_URL=https://rdp.example.com/guacamole/`.
6. Update the CSP `connect-src` hostname in `Caddyfile` to the same public
   hostname.
7. Restrict outbound traffic so only DNS, required update endpoints, and the
   approved RDP destination networks are reachable.

Run `docker compose config --quiet` again after these changes. Terminate TLS at
only one explicitly managed layer, preserve WebSocket upgrade headers, and do
not place the portal or Guacamole direct ports behind a public listener.

## Persistent data and backup

Compose creates these named volumes:

| Volume suffix | Contents |
|---|---|
| `database` | Users, targets, policies, assignments, sessions, and audit data |
| `caddy_data` | Caddy certificates and state |
| `caddy_config` | Caddy runtime configuration |
| `recordings` | Guacd recording path; recording is disabled by current policy |

Create a consistent PostgreSQL backup:

```sh
docker compose exec -T database pg_dump -U vpn -d vpn -Fc > vpn-database.dump
```

Restore into an empty, compatible database:

```sh
docker compose exec -T database pg_restore -U vpn -d vpn --clean --if-exists < vpn-database.dump
```

Test restoration away from production. Back up `.env` separately using an
encrypted secret store; a database dump alone is not sufficient for recovery.

## Upgrade and rollback

The Compose file pins third-party images by digest. Review release notes and
update those digests deliberately. Before an upgrade, back up PostgreSQL and
record the current Git commit:

```sh
git rev-parse HEAD
docker compose exec -T database pg_dump -U vpn -d vpn -Fc > vpn-before-upgrade.dump
git pull --ff-only
docker compose config --quiet
docker compose build --pull
docker compose up -d
docker compose ps
```

Database migrations run automatically and may make application rollback
unsafe. Restore the pre-upgrade database backup when a migration is not
backward-compatible.

## Operations and troubleshooting

```sh
# Current service state
docker compose ps

# Recent logs
docker compose logs --tail=200 portal caddy guacamole guacd database

# Restart one service
docker compose restart portal

# Recreate after environment changes
docker compose up -d --force-recreate portal guacamole

# Stop containers without deleting persistent data
docker compose down
```

Common failures:

- **Compose reports a missing variable:** replace every placeholder in `.env`.
- **Login origin is rejected:** `PORTAL_ALLOWED_ORIGIN` must exactly match the
  browser origin, including scheme and port.
- **Guacamole launch is invalid or expired:** confirm both containers use the
  same JSON secret and that host clocks are synchronized.
- **RDP certificate mismatch:** obtain the current destination certificate
  fingerprint and approve the change through the management process; never
  bypass pinning.
- **RDP connection timeout:** verify routing/firewall access from `guacd` to the
  target on TCP 3389 and confirm NLA is enabled on Windows.

To remove containers and all persistent data, use the following destructive
command only when a backup is confirmed:

```sh
docker compose down --volumes
```
