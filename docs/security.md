# Security Architecture — Vault System

> **Ultimo aggiornamento**: 2026-02-23

## Panoramica

Homun implementa un sistema di vault crittografato per la gestione sicura di secrets (API key, token, password). L'architettura è progettata secondo il principio "defense in depth" con multipli livelli di protezione.

## Architettura

```
┌─────────────────────────────────────────────────────────────────┐
│                         VAULT ARCHITECTURE                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐     ┌───────────────┐     ┌───────────────┐  │
│  │   LLM Tool   │────▶│  VaultTool    │────▶│ Encrypted     │  │
│  │  (vault)     │     │ (store/get)   │     │ Secrets       │  │
│  └──────────────┘     └───────────────┘     │ (.enc file)   │  │
│                                             └───────┬───────┘  │
│                                                     │          │
│  ┌──────────────┐     ┌───────────────┐            │          │
│  │   Gateway    │────▶│ Token/Key     │────────────┘          │
│  │ (channels)   │     │ Resolution    │                       │
│  └──────────────┘     └───────────────┘                       │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                    MASTER KEY STORAGE                     │  │
│  │  ┌─────────────────┐    ┌─────────────────────────────┐  │  │
│  │  │  OS Keychain    │ OR │  File (~/.homun/master.key) │  │  │
│  │  │  - macOS        │    │  Permissions: 0600          │  │  │
│  │  │  - Linux secret │    │  (fallback headless)        │  │  │
│  │  │  - Windows cred │    │                             │  │  │
│  │  └─────────────────┘    └─────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Componenti

### 1. Encrypted Secrets (`src/storage/secrets.rs`)

Core module per la crittografia e persistenza dei secrets.

**Caratteristiche:**
- **Algoritmo**: AES-256-GCM (Authenticated Encryption)
- **Nonce**: Randomico per ogni operazione di encryption
- **Storage**: `~/.homun/secrets.enc` (JSON crittografato)
- **Memoria**: Azzerata dopo l'uso (`zeroize` crate)

**Struttura file crittografato:**
```json
{
  "version": 1,
  "nonce": "base64_encoded_nonce",
  "ciphertext": "base64_encoded_ciphertext_with_auth_tag"
}
```

### 2. Vault Tool (`src/tools/vault.rs`)

Tool LLM per la gestione dei secrets da conversazione.

**Azioni disponibili:**
| Azione | Descrizione | Parametri |
|--------|-------------|-----------|
| `store` | Salva un secret | `key`, `value` |
| `retrieve` | Recupera un secret | `key` |
| `list` | Lista delle chiavi | — |
| `delete` | Elimina un secret | `key` |

**Esempio di utilizzo:**
```
Utente: "La mia password WiFi è SuperSecret123, memorizzala"
LLM: *chiama vault store(key="wifi_password", value="SuperSecret123")*
     "Memorizzato come vault://wifi_password"
```

### 3. Token Resolution (`src/agent/gateway.rs`)

Sistema automatico di risoluzione dei token per canali e provider.

**Flusso:**
1. Legge config.toml
2. Se token = `***ENCRYPTED***` o vuoto
3. Query al vault con chiave appropriata
4. Sostituisce con valore reale

### 4. Web UI (`static/js/vault.js`, `src/web/api.rs`)

Interfaccia web per gestire i secrets.

**Endpoint API:**
- `GET /api/v1/vault` — Lista chiavi
- `POST /api/v1/vault` — Salva secret
- `DELETE /api/v1/vault/:key` — Elimina secret

## Namespace delle Chiavi

```
provider.{name}.api_key    → API key per provider LLM
channel.{name}.token       → Token per canale di comunicazione
vault.{user_key}           → Secrets generici (via LLM tool)
```

**Esempi:**
```
provider.openai.api_key      → sk-...
provider.anthropic.api_key   → sk-ant-...
channel.telegram.token       → 123456:ABC...
channel.discord.token        → NTIzNjg...
vault.wifi_password          → SuperSecret123
vault.email_password         → myemailpass
```

## Master Key Storage

### Priorità

1. **OS Keychain** (preferito)
   - macOS: Keychain Access
   - Linux: Secret Service (GNOME Keyring, KWallet)
   - Windows: Credential Manager

2. **File-based** (fallback)
   - Path: `~/.homun/master.key`
   - Permessi: `0600` (solo owner)
   - Usato su: server headless, Docker, WSL senza GUI

### Generazione

La master key viene generata automaticamente al primo avvio:
- 32 byte randomici (256 bit)
- Codificati in Base64
- Mai esposta in logs o errori

## Configurazione

### config.toml

```toml
# Token nel vault (marker)
[channels.telegram]
enabled = true
token = "***ENCRYPTED***"  # Risolto da vault

# Token in chiaro (non raccomandato)
[channels.discord]
enabled = true
token = "NTIzNjg..."  # Funziona ma meno sicuro

# Provider API keys
[providers.openrouter]
api_key = "***ENCRYPTED***"  # Risolto da vault
```

### Primo Setup

```bash
# 1. Avvia Homun (crea automaticamente la master key)
homun gateway

# 2. Memorizza i secrets via Web UI (http://localhost:18080/vault)
# O via LLM:
# "Memorizza il mio token Telegram: 123456:ABC..."
```

## Proprietà di Sicurezza

### Crittografia

| Proprietà | Implementazione |
|-----------|-----------------|
| Algoritmo | AES-256-GCM |
| Dimensione chiave | 256 bit |
| Nonce | 96 bit randomico per encryption |
| Authentication | GCM tag (128 bit) |
| KDF | Non necessario (chiave già 256 bit) |

### Protezione a Livello File

```bash
$ ls -la ~/.homun/
-rw-------  1 user  staff    512 Feb 23 10:00 secrets.enc
-rw-------  1 user  staff     64 Feb 23 09:00 master.key
```

### Protezione in Memoria

- `Zeroizing<[u8; 32]>` per la master key
- Valori dei secrets mai loggati
- Context dell'LLM usa solo riferimenti `vault://key`

### Atomic Writes

```rust
// Write su file temporaneo + rename atomico
let temp_path = path.with_extension("tmp");
fs::write(&temp_path, encrypted_data)?;
fs::rename(&temp_path, &path)?;  // Atomic on POSIX
```

## Minacce Mitigate

| Minaccia | Mitigazione |
|----------|-------------|
| Accesso fisico al disco | Encryption AES-256-GCM |
| Memory dump | Zeroize dopo uso |
| File permission leak | 0600 permissions |
| Backup non sicuri | File crittografato, chiave separata |
| Insider threat | OS keychain (richiede auth utente) |
| Tampering | GCM authentication tag |

## Rotazione della Master Key

**Nota**: La rotazione manuale della master key richiede re-encryption.

```bash
# 1. Backup dei secrets (decrittografati)
homun vault export > secrets_backup.json

# 2. Rimuovi vecchia master key
rm ~/.homun/master.key
# O rimuovi da OS keychain

# 3. Riavvia per generare nuova chiave
homun gateway

# 4. Re-importa i secrets
homun vault import < secrets_backup.json
```

## Best Practices

1. **Usa sempre il vault** per token e API key
2. **Non committare** `secrets.enc` o `master.key` in git
3. **Backup** della master key separatamente
4. **Verifica** i permessi dei file (0600)
5. **Usa OS keychain** quando possibile (più sicuro)

## Troubleshooting

### "Failed to access OS keychain"

```
# Su Linux, assicurati che secret service sia attivo
echo "test" | secret-tool store --label="test" test test

# Su server headless, usa file-based fallback
# (automatico se keychain non disponibile)
```

### "Failed to decrypt secrets"

```
# Master key corrotta o cambiata?
# 1. Verifica che master.key esista
ls -la ~/.homun/master.key

# 2. Se hai backup della chiave, ripristinala
cp backup/master.key ~/.homun/

# 3. Altrimenti, devi re-inserire i secrets
```

### Token non risolto

```bash
# Verifica che il secret esista nel vault
homun vault list

# Dovresti vedere:
# - channel.telegram.token
# - provider.openrouter.api_key
```

## Riferimenti

- `src/storage/secrets.rs` — Implementazione encryption
- `src/tools/vault.rs` — LLM tool
- `src/agent/gateway.rs` — Token resolution
- `src/web/api.rs` — REST API
- `static/js/vault.js` — Web UI
