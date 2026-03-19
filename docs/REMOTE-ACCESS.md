# Remote Access Guide

Homun runs on `localhost:18443` by default. To access it remotely, use one of these patterns.

## 1. SSH Tunnel (simplest)

No configuration needed. Forward the port over SSH:

```bash
ssh -L 18443:localhost:18443 user@your-server
```

Then open `https://localhost:18443` in your local browser. The connection is encrypted by SSH.

**Pros**: Zero config, works with self-signed TLS cert.
**Cons**: Requires SSH access, tunnel must stay open.

## 2. Tailscale Serve (recommended)

If you use [Tailscale](https://tailscale.com), expose Homun to your tailnet:

```bash
tailscale serve https / http://localhost:18443
```

Access via `https://your-machine.tail-net-name.ts.net`. Tailscale handles TLS and auth.

**Pros**: End-to-end encrypted, no ports exposed to internet, works across devices.
**Cons**: Requires Tailscale on both machines.

## 3. Reverse Proxy (Caddy / nginx)

For public-facing setups, use a reverse proxy with a real domain and TLS cert.

### Caddy

```
homun.example.com {
    reverse_proxy localhost:18443 {
        transport http {
            tls_insecure_skip_verify
        }
    }
}
```

### nginx

```nginx
server {
    listen 443 ssl;
    server_name homun.example.com;

    ssl_certificate /etc/letsencrypt/live/homun.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/homun.example.com/privkey.pem;

    location / {
        proxy_pass https://127.0.0.1:18443;
        proxy_ssl_verify off;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header Host $host;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

**Important**: Enable X-Forwarded-For awareness in Homun so rate limiting uses the real client IP:

```toml
[channels.web]
trust_x_forwarded_for = true
```

## Security Checklist

When exposing Homun remotely, enable these protections:

| Setting | Config | Default | Recommendation |
|---------|--------|---------|----------------|
| Device approval | `require_device_approval = true` | `false` | Enable for remote access |
| Session lifetime | `session_ttl_secs = 3600` | `86400` (24h) | Reduce for remote |
| XFF trust | `trust_x_forwarded_for = true` | `false` | Enable behind reverse proxy |
| Rate limiting | `auth_rate_limit_per_minute = 3` | `5` | Tighten for remote |

### Example remote config

```toml
[channels.web]
host = "127.0.0.1"
port = 18443
trust_x_forwarded_for = true
session_ttl_secs = 3600
require_device_approval = true
auth_rate_limit_per_minute = 3
```

## Device Approval Flow

When `require_device_approval = true`:

1. User logs in with correct credentials from a new browser
2. Login returns `device_approval_required` with a 6-digit code logged to the server
3. User enters the code on the login page (or approves from an existing session via Settings > Devices)
4. Device is marked as trusted — future logins from the same browser succeed immediately

### Managing devices

- **List**: `GET /api/v1/devices`
- **Approve from session**: `POST /api/v1/devices/{id}/approve`
- **Revoke**: `DELETE /api/v1/devices/{id}`
- **Approve via code**: `POST /api/auth/device-approve` (no session needed)

### How fingerprinting works

Device fingerprint = SHA-256 hash of `user_id + User-Agent`. This means:
- Same browser on same OS = same device (even across IPs)
- Different browser = different device
- Browser update that changes User-Agent = new device (re-approval needed)
