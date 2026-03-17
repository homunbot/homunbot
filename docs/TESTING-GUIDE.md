# Homun — Guida al Testing

> Last updated: 2026-03-12
> Binary: `cargo run` (debug) o `cargo build --release --features full`

---

## Quick Start

```bash
# 1. Compilare (prima volta: ~3 min)
cargo build --features full

# 2. Configurare un provider LLM
homun provider add anthropic --api_key sk-ant-xxx

# 3. Chat interattiva
homun chat

# 4. Messaggio one-shot
homun chat -m "Ciao, come ti chiami?"

# 5. Avviare il gateway (web UI + canali + cron)
homun gateway
# Dashboard: https://localhost
```

---

## 1. Chat & Agent Loop

L'agent funziona in loop ReAct: ragiona, chiama tool, osserva il risultato, ripete.

### Test base

```bash
# Chat interattiva
homun chat

# One-shot
homun chat -m "Quanto fa 17 * 23?"
homun chat -m "Che giorno e' oggi?"
```

### Test tool calling

```
> Elenca i file nella directory corrente
# Dovrebbe usare shell tool (ls) o file tool (list_dir)

> Leggi il file Cargo.toml e dimmi la versione
# Usa file tool (read_file)

> Scrivi un file test.txt con "ciao mondo"
# Usa file tool (write_file)

> Cerca su internet le ultime notizie su Rust
# Usa web_search (richiede Brave API key configurata)
```

### Test multi-iterazione

```
> Trova tutti i file .rs nel progetto, conta quante righe hanno in totale, e dimmi qual e' il piu' grande
# Richiede piu' tool call in sequenza: list_dir + shell (wc -l)
```

---

## 2. Provider LLM

### Gestione provider

```bash
# Lista provider configurati
homun provider list

# Aggiungere un provider
homun provider add anthropic --api_key sk-ant-xxx
homun provider add openrouter --api_key sk-or-xxx
homun provider add ollama --api_base http://localhost:11434/v1

# Rimuovere
homun provider remove deepseek
```

### Modelli supportati

| Provider | Prefisso modello | Esempio |
|----------|-----------------|---------|
| Anthropic | `anthropic/`, `claude` | `anthropic/claude-sonnet-4-20250514` |
| OpenAI | `openai/`, `gpt-`, `o1-`, `o3-` | `openai/gpt-4o` |
| OpenRouter | (qualsiasi) | `meta-llama/llama-3-70b` |
| Ollama | `ollama/` | `ollama/llama3` |
| DeepSeek | `deepseek` | `deepseek-chat` |
| Groq | `groq/` | `groq/llama3-70b` |
| Gemini | `gemini` | `gemini-pro` |

### Test cambio modello

```bash
# Via config
homun config set agent.model "anthropic/claude-sonnet-4-20250514"

# Via Web UI
# Dashboard > Settings > Model
```

---

## 3. Web UI

Avviare il gateway e aprire il browser:

```bash
homun gateway
# Apri https://localhost
```

Nota: per default la Web UI è disponibile su `https://localhost`. Per domini custom, configurare `[channels.web] domain` o usare il Docker stack con `HOMUN_DOMAIN`.

### Pagine disponibili

| Pagina | URL | Cosa testare |
|--------|-----|-------------|
| Dashboard | `/` | Uptime, canali attivi, modello, status |
| Chat | `/chat` | Inviare messaggi, vedere streaming, tool call |
| Skills | `/skills` | Installare/rimuovere skill, cercare su ClawHub |
| Memory | `/memory` | Statistiche, cercare memorie, vedere score |
| Vault | `/vault` | Salvare/recuperare/eliminare secrets |
| Logs | `/logs` | Log in tempo reale, filtro per livello |
| Permissions | `/permissions` | Livello autonomia, pending approvals |
| Account | `/account` | Utenti, identita' canale, token webhook |
| Setup | `/setup` | Wizard configurazione iniziale |
| Approvals | `/approvals` | Coda approvazioni pending |

### Test Web Chat

1. Aprire `/chat`
2. Scrivere un messaggio e inviare
3. Verificare: risposta in streaming, tool call visibili, history mantenuta

### Smoke via Playwright MCP/CLI

Questi script riusano il wrapper Playwright CLI gia' usato nella toolchain Codex, senza introdurre `@playwright/test`.

Prerequisiti:

```bash
command -v npx
test -x "$HOME/.codex/skills/playwright/scripts/playwright_cli.sh" || test -x "./scripts/playwright_cli.sh"
```

Gli script caricano automaticamente `./.env` se presente.
Per default la console resta compatta e il dettaglio completo del CLI finisce in `output/playwright/*.cli.log`.
Per vedere tutto anche a terminale:

```bash
HOMUN_E2E_VERBOSE=1 ./scripts/e2e_chat_suite.sh
```

Smoke Web UI login/setup/chat shell:

```bash
HOMUN_E2E_USERNAME=admin \
HOMUN_E2E_PASSWORD=changeme123 \
./scripts/e2e_webui_smoke.sh
```

Smoke Browser settings + prerequisites check:

```bash
HOMUN_E2E_USERNAME=admin \
HOMUN_E2E_PASSWORD=changeme123 \
./scripts/e2e_browser_smoke.sh
```

Prompt opzionale per verificare anche il composer chat:

```bash
HOMUN_E2E_USERNAME=admin \
HOMUN_E2E_PASSWORD=changeme123 \
HOMUN_E2E_CHAT_PROMPT="Rispondi con la parola smoke" \
./scripts/e2e_webui_smoke.sh
```

Smoke `send -> run attiva -> stop` per la chat web:

```bash
HOMUN_E2E_USERNAME=admin \
HOMUN_E2E_PASSWORD=changeme123 \
./scripts/e2e_chat_send_stop.sh
```

Prompt piu' lungo/pesante opzionale se il modello risponde troppo in fretta:

```bash
HOMUN_E2E_USERNAME=admin \
HOMUN_E2E_PASSWORD=changeme123 \
HOMUN_E2E_CHAT_PROMPT="Produce exactly 200 numbered lines. Keep writing until all 200 are complete." \
./scripts/e2e_chat_send_stop.sh
```

Smoke multi-sessione:

```bash
HOMUN_E2E_USERNAME=admin \
HOMUN_E2E_PASSWORD=changeme123 \
./scripts/e2e_chat_multi_session.sh
```

Smoke restore dopo reload durante una run:

```bash
HOMUN_E2E_USERNAME=admin \
HOMUN_E2E_PASSWORD=changeme123 \
./scripts/e2e_chat_restore_run.sh
```

Smoke allegati documento:

```bash
HOMUN_E2E_USERNAME=admin \
HOMUN_E2E_PASSWORD=changeme123 \
./scripts/e2e_chat_attachment_smoke.sh
```

Smoke MCP picker:

```bash
HOMUN_E2E_USERNAME=admin \
HOMUN_E2E_PASSWORD=changeme123 \
./scripts/e2e_chat_mcp_picker_smoke.sh
```

Smoke browser tool flow deterministico via chat:

```bash
HOMUN_E2E_USERNAME=admin \
HOMUN_E2E_PASSWORD=changeme123 \
./scripts/e2e_browser_tool_flow.sh
```

Questo smoke non dipende da siti esterni: genera una fixture HTML self-contained come `data:` URL, forza l'agente a usare il tool `browser`, e verifica sia il token finale sia la presenza di activity card browser nella chat.

Suite completa:

```bash
HOMUN_E2E_USERNAME=admin \
HOMUN_E2E_PASSWORD=changeme123 \
./scripts/e2e_chat_suite.sh
```

Artifact prodotti:

- `output/playwright/webui-chat-smoke.snapshot.txt`
- `output/playwright/webui-chat-smoke.png`
- `output/playwright/e2e_webui_smoke.cli.log`
- `output/playwright/browser-smoke.snapshot.txt`
- `output/playwright/browser-smoke.png`
- `output/playwright/e2e_browser_smoke.cli.log`
- `output/playwright/chat-send-stop.snapshot.txt`
- `output/playwright/chat-send-stop.png`
- `output/playwright/e2e_chat_send_stop.cli.log`
- `output/playwright/chat-multi-session.snapshot.txt`
- `output/playwright/chat-multi-session.png`
- `output/playwright/e2e_chat_multi_session.cli.log`
- `output/playwright/chat-restore-run.snapshot.txt`
- `output/playwright/chat-restore-run.png`
- `output/playwright/e2e_chat_restore_run.cli.log`
- `output/playwright/chat-attachment.snapshot.txt`
- `output/playwright/chat-attachment.png`
- `output/playwright/e2e_chat_attachment_smoke.cli.log`
- `output/playwright/chat-mcp-picker.snapshot.txt`
- `output/playwright/chat-mcp-picker.png`
- `output/playwright/e2e_chat_mcp_picker_smoke.cli.log`
- `output/playwright/browser-tool-flow.snapshot.txt`
- `output/playwright/browser-tool-flow.png`
- `output/playwright/e2e_browser_tool_flow.cli.log`

Workflow GitHub Actions manuale/opzionale:

- file: `.github/workflows/e2e-smoke.yml`
- trigger: `workflow_dispatch`
- segreti richiesti: `HOMUN_E2E_USERNAME`, `HOMUN_E2E_PASSWORD`
- input principali: `base_url`, `script`, `verbose`

### Test Memory Search

1. Aprire `/memory`
2. Usare la barra di ricerca
3. Verificare: risultati con score colorato (verde >= 50%, giallo < 50%)

### API REST

```bash
# Health check
curl https://localhost/api/health

# Status completo
curl https://localhost/api/status

# Configurazione
curl https://localhost/api/config

# Ricerca memorie (hybrid search)
curl "https://localhost/api/v1/memory/search?q=test&limit=5"

# Lista skills
curl https://localhost/api/v1/skills

# Lista provider
curl https://localhost/api/v1/providers
```

---

## 4. Skills

### Gestione skill

```bash
# Lista installate
homun skills list

# Cercare su GitHub
homun skills search "web scraping"

# Cercare su ClawHub marketplace (3000+ skills)
homun skills hub "python data"

# Installare da GitHub
homun skills add owner/repo

# Installare da ClawHub
homun skills add clawhub:owner/skill-name

# Info su una skill
homun skills info nome-skill

# Rimuovere
homun skills remove nome-skill
```

### Test skill execution

1. Installare una skill con script
2. In chat: chiedere qualcosa che attivi la skill
3. Verificare: l'agent carica il body SKILL.md e esegue lo script

---

## 5. Memory & Embedding

### Status memoria

```bash
homun memory status
```

Output atteso:
```
Memory Statistics:
  Total chunks: 42
  Memory file: ~/.homun/MEMORY.md (1.2 KB)
  History file: ~/.homun/HISTORY.md (3.4 KB)
  Daily files: 5
  Vector index: 42 vectors (384-dim)
  Last consolidation: 2026-03-03 14:22
```

### Test consolidation

1. Avviare una chat lunga (10+ messaggi)
2. La consolidation dovrebbe attivarsi automaticamente
3. Verificare che `~/.homun/MEMORY.md` viene aggiornato
4. Verificare che `~/.homun/memory/YYYY-MM-DD.md` viene creato

### Test ricerca ibrida

In chat:
```
> Cosa ricordi di me?
# L'agent cerca automaticamente le memorie rilevanti (top 5)
# e le inietta nel context prima di rispondere
```

Via API:
```bash
curl "https://localhost/api/v1/memory/search?q=preferenze&limit=5"
```

### Configurazione embedding

```toml
# ~/.homun/config.toml
[memory]
embedding_provider = "ollama"   # Ollama (default, free, local)
# embedding_provider = "openai"  # OpenAI API (richiede api_key)
```

### Reset memoria

```bash
homun memory reset         # chiede conferma
homun memory reset --force # senza conferma
```

---

## 6. Vault (Secrets Crittografati)

### Via CLI (chat)

```
> Salva nel vault la chiave "github_token" con valore "ghp_xxx123"
> Recupera dal vault "github_token"
> Lista tutti i secrets nel vault
> Elimina dal vault "github_token"
```

### Via Web UI

1. Aprire `/vault`
2. Aggiungere un secret (nome + valore)
3. Verificare che il valore e' redatto nella lista
4. Recuperare il secret

### Proprieta' di sicurezza

- Crittografia: AES-256-GCM con nonce random
- Master key: OS Keychain (macOS Keychain / Linux Secret Service)
- File: `~/.homun/secrets.enc` con permessi 0600
- 2FA opzionale (TOTP) per accesso vault

---

## 7. Cron Jobs

```bash
# Lista job attivi
homun cron list

# Aggiungere job con cron expression
homun cron add --name "report-mattina" \
  --message "Dammi un riepilogo delle news di oggi" \
  --cron "0 9 * * *"

# Aggiungere job con intervallo (secondi)
homun cron add --name "heartbeat" \
  --message "Controlla lo stato del sistema" \
  --every 3600

# Rimuovere
homun cron remove <id>
```

> I cron job funzionano solo in **gateway mode** (`homun gateway`).

---

## 8. Canali di Comunicazione

### Telegram

```toml
# ~/.homun/config.toml
[channels.telegram]
enabled = true
token = "123456:ABC..."       # o ***ENCRYPTED*** se nel vault
allow_from = ["123456789"]    # ID utenti autorizzati
```

```bash
homun gateway
# Inviare messaggi dal bot Telegram
```

### Discord

```toml
[channels.discord]
enabled = true
token = "MTxx..."
```

### WhatsApp

```toml
[channels.whatsapp]
enabled = true
# Il pairing avviene via Web UI: /setup > WhatsApp > QR code
```

### Email

```toml
[channels.email]
enabled = true
imap_host = "imap.gmail.com"
imap_port = 993
smtp_host = "smtp.gmail.com"
smtp_port = 587
username = "you@gmail.com"
password = "***ENCRYPTED***"
```

### Slack

```toml
[channels.slack]
enabled = true
bot_token = "xoxb-..."
signing_secret = "..."
```

---

## 9. Browser Automation

Richiede feature `browser` e Chrome/Chromium installato.

```toml
[browser]
enabled = true
headless = false          # true per server senza display
# executable = "/usr/bin/google-chrome"  # auto-detect di default
```

### Test in chat

```
> Apri https://example.com e dimmi cosa c'e' scritto
> Vai su https://news.ycombinator.com e dimmi i primi 3 titoli
> Fai uno screenshot della pagina corrente
```

---

## 10. MCP (Model Context Protocol)

### Configurare un server MCP

```bash
# Via CLI
homun mcp add filesystem --transport stdio \
  --command npx --args "@anthropic/mcp-filesystem /tmp"

# Via config
```

```toml
[mcp.filesystem]
transport = "stdio"
command = "npx"
args = ["@anthropic/mcp-filesystem", "/tmp"]
enabled = true
```

### Gestione

```bash
homun mcp list
homun mcp toggle filesystem
homun mcp remove filesystem
```

### Test

In chat, i tool MCP appaiono automaticamente con prefisso `servername_`:
```
> Elenca i file in /tmp usando il server MCP filesystem
```

---

## 10b. Sandbox Execution

Il sistema sandbox isola l'esecuzione di shell, MCP stdio e skill scripts.

### Test unitari (31 test)

```bash
cargo test -- sandbox
```

### Test integrazione Linux (richiede bwrap)

```bash
# Su Linux con bubblewrap installato
sudo apt-get install bubblewrap
cargo test --test sandbox_linux_native -- --nocapture
```

Test inclusi: probe bwrap, echo sandboxed, env sanitization, network isolation, prlimit memory, workspace mount, rootfs read-only.

### Test integrazione runtime image (richiede Docker)

```bash
# Build della baseline image
./scripts/build_sandbox_runtime_image.sh

# Esegui i test
cargo test --test sandbox_runtime_image -- --nocapture
```

Test inclusi: build baseline, verifica node/python/bash/tsx, esecuzione sandboxed nella baseline.

### Test E2E cross-platform

```bash
cargo test --test sandbox_e2e -- --nocapture
```

Test portabili che funzionano su macOS/Linux/Windows. I test saltano gracefully se Docker o bwrap non sono disponibili.

### CI

Il workflow `.github/workflows/sandbox-validation.yml` esegue automaticamente su push/PR ai file sandbox:
- Linux native (Ubuntu + bwrap)
- Runtime image (Ubuntu + Docker)
- E2E su Linux, Windows, macOS

---

## 11. Utenti & Permessi

### Gestione utenti

```bash
# Creare utente admin
homun users add fabio --admin

# Creare utente normale
homun users add alice

# Collegare identita' Telegram
homun users link --user fabio --channel telegram --id 123456789

# Creare token per webhook
homun users token --user fabio --name ci-webhook

# Info utente
homun users info fabio

# Lista utenti
homun users list
```

### Livelli di autonomia

| Livello | Comportamento |
|---------|--------------|
| `locked` | L'agent non puo' eseguire tool senza approvazione |
| `approval` | Tool pericolosi richiedono approvazione (default) |
| `autonomous` | L'agent agisce liberamente |

Configurabile via Web UI (`/permissions`) o config:
```toml
[permissions.approval]
level = "approval"
```

---

## 12. Configurazione

### Dot-path get/set

```bash
# Leggere un valore
homun config get agent.model
homun config get channels.telegram.enabled

# Scrivere un valore
homun config set agent.model "anthropic/claude-sonnet-4-20250514"
homun config set agent.temperature 0.7
homun config set channels.telegram.enabled true

# Mostrare tutta la config
homun config show

# Path del file
homun config path
# ~/.homun/config.toml
```

### TUI di configurazione

```bash
homun config
# Apre TUI con tab: Settings, Providers, WhatsApp, Skills, MCP
```

---

## 13. Service (auto-start)

```bash
# Installare come servizio utente (systemd su Linux, launchd su macOS)
homun service install

# Gestire
homun service start
homun service stop
homun service status
homun service uninstall
```

---

## 14. Compilazione & Feature Flags

### Profili di build

```bash
# Minimo (solo CLI chat, ~15MB)
cargo build --release

# Con Telegram
cargo build --release --features channel-telegram

# Gateway completo (tutti i canali + embeddings + MCP)
cargo build --release --features gateway

# Full (gateway + browser, ~50MB)
cargo build --release --features full
```

### Feature disponibili

| Feature | Cosa include |
|---------|-------------|
| `cli` | CLI interattiva (default) |
| `web-ui` | Dashboard web (default) |
| `channel-telegram` | Bot Telegram |
| `channel-discord` | Bot Discord |
| `channel-whatsapp` | Client WhatsApp nativo |
| `channel-email` | IMAP/SMTP |
| `embeddings` | Vector search (Ollama/OpenAI + USearch) |
| `browser` | Browser automation (CDP) |
| `mcp` | Model Context Protocol |
| `vault-2fa` | 2FA per il vault |
| `gateway` | Tutti i canali + embeddings + MCP |
| `full` | Gateway + browser |

---

## 15. Troubleshooting

### Log verbosi

```bash
RUST_LOG=debug homun chat
RUST_LOG=debug homun gateway
RUST_LOG=homun=trace homun chat  # solo log di homun
```

### Problemi comuni

| Problema | Soluzione |
|----------|----------|
| "No provider configured" | `homun provider add anthropic --api_key sk-ant-xxx` |
| Tool call fallisce | Verificare permessi in `/permissions` |
| Memory search vuota | Fare una chat lunga per triggare consolidation |
| WhatsApp non si connette | Usare Web UI `/setup` per il pairing QR |
| Embedding model download lento | Prima esecuzione scarica ~30MB, poi e' cached |
| Browser tool non disponibile | Compilare con `--features browser`, Chrome installato |
| "Database locked" | Un'altra istanza di homun e' in esecuzione |

### File importanti

```
~/.homun/
  config.toml           # Configurazione
  homun.db              # Database SQLite
  secrets.enc           # Vault crittografato
  MEMORY.md             # Memoria lungo termine
  HISTORY.md            # Log eventi
  memory/               # File giornalieri
    2026-03-03.md
  brain/
    USER.md             # Profilo utente
    SOUL.md             # Identita' dell'agent
    INSTRUCTIONS.md     # Istruzioni apprese
  skills/               # Skill installate
  memory.usearch        # Indice vettoriale HNSW
```
