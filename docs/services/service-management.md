# Service Management

## Purpose

This subsystem owns how Homun is installed and managed as a user service outside the interactive shell. It is separate from the gateway logic itself and covers OS-level startup helpers and process management expectations.

## Primary Code

- `src/service/mod.rs`
- `src/service/systemd.rs`
- `src/service/launchd.rs`
- `src/main.rs`

## Supported Targets

Current code supports:

- Linux user services through systemd
- macOS user agents through launchd

Unsupported OSes fail explicitly for these commands.

## CLI Surface

`homun service` currently supports:

- `install`
- `uninstall`
- `start`
- `stop`
- `status`

The generated service runs `homun gateway`.

## Linux Details

The systemd unit is a user service under `~/.config/systemd/user/homun.service`. The generated unit includes:

- restart on failure
- `HOME` and `RUST_LOG`
- some service hardening flags
- read/write access for `~/.homun`

## Runtime Process Management

Separate from OS service helpers, `src/main.rs` also manages:

- gateway PID file
- stop/restart commands
- stale PID cleanup

That is runtime-level process hygiene, not OS service installation.

## Failure Modes And Limits

- no Windows service management path exists here
- service install support is intentionally user-level, not system-wide
- service hardening exists but is not the same thing as the still-partial sandbox hardening work

## Change Checklist

Update this document when you change:

- supported service managers
- generated service content
- gateway process management behavior
- deployment assumptions for long-running runtime installs
