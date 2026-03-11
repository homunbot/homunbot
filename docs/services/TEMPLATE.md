# Service Template

Use this template for any new service document in this folder.

## Purpose

One paragraph on what the subsystem owns and what it does not own.

## Primary Code

- `src/...`
- `src/...`

## Runtime Role

- entrypoints
- startup path
- feature gates
- background tasks

## Config Ownership

- config sections
- secrets/vault usage
- which changes are hot-reloaded and which require restart

## Persistent State

- SQLite tables
- files under `~/.homun/`
- caches / indexes

## Interfaces

- inbound inputs
- outbound APIs/events/tools
- web endpoints or CLI commands

## Failure Modes And Limits

- common failure paths
- security constraints
- partial implementations

## Tests And Verification

- unit/integration tests
- smoke tests
- manual checks

## Change Checklist

- what else must be updated when this subsystem changes
