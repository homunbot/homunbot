# Getting Started with Homun

This guide walks you from zero to a working Homun instance with your first chat, your first channel, and your first automation.

**Time estimate**: ~15 minutes.

---

## Step 1: Install Homun

Choose one method:

### Option A: Docker (recommended)

```bash
git clone https://github.com/homunbot/homun.git
cd homun
cp .env.example .env
docker compose up -d
```

Homun is now running at **https://localhost**.

### Option B: Build from source

Requires Rust 1.75+.

```bash
git clone https://github.com/homunbot/homun.git
cd homun
cargo install --path . --features full
```

### Option C: Pre-built binary

Download from [GitHub Releases](https://github.com/homunbot/homun/releases), verify the SHA256 checksum, and place the binary in your PATH.

---

## Step 2: First boot and setup wizard

**Docker users**: open **https://localhost** in your browser. Accept the self-signed certificate warning.

**Binary users**: run the gateway first:

```bash
homun gateway
```

Then open **https://localhost** in your browser.

On first boot, Homun redirects you to the **setup wizard**:

1. **Create admin account** -- choose a username and password
2. **Choose your LLM provider** -- select one (Anthropic, OpenAI, Ollama, etc.) and enter the API key
3. **Test the connection** -- the wizard verifies your API key works
4. **Done** -- you're redirected to the dashboard

> **Tip**: If you want fully free/local operation, install [Ollama](https://ollama.com), pull a model (`ollama pull qwen3:latest`), and select "Ollama" in the wizard. Docker users can enable the Ollama sidecar: `docker compose --profile with-ollama up -d`.

---

## Step 3: Send your first message

### Via Web UI

Click **Chat** in the sidebar. Type a message and press Enter. Homun responds with streaming text. You'll see a tool timeline when Homun uses tools (web search, file operations, etc.).

### Via CLI

```bash
homun chat
```

This opens an interactive REPL. Type your message and press Enter.

For a single response without entering the REPL:

```bash
homun chat -m "What day is it today?"
```

---

## Step 4: Connect your first channel

Channels let you talk to Homun from Telegram, Discord, WhatsApp, Slack, or Email.

### Example: Telegram

1. Create a bot with [@BotFather](https://t.me/BotFather) on Telegram and copy the token
2. Open the Web UI and go to **Channels**
3. Click the **Telegram** card
4. Enter the bot token and your Telegram user ID (for access control)
5. Toggle **Enabled** and save
6. Homun connects automatically -- send a message to your bot

> **Tip**: Find your Telegram user ID by messaging [@userinfobot](https://t.me/userinfobot).

Other channels follow the same pattern: get credentials from the platform, configure in the Channels page, and enable.

---

## Step 5: Create your first automation

Automations let Homun perform tasks on a trigger (schedule, webhook, or manual).

### Via Web UI (visual builder)

1. Go to **Automations** in the sidebar
2. Click **New Automation**
3. Use the visual flow builder to add nodes:
   - **Trigger** node: choose "Schedule" and set a cron (e.g., every day at 9am)
   - **LLM** node: add a prompt (e.g., "Summarize the top 3 tech news stories today")
   - **Message** node: send the result to your Telegram
4. Save and enable the automation

### Via CLI (cron)

For simpler scheduled tasks:

```bash
homun cron add --schedule "0 9 * * *" --message "Summarize today's top tech news and send it to me"
```

This creates a daily 9am task. List your cron jobs with `homun cron list`.

---

## Step 6: Teach Homun about you

Homun has a persistent memory system. It remembers facts about you across sessions.

Tell it things naturally in chat:

- "Remember that I prefer Python over JavaScript"
- "My timezone is Europe/Rome"
- "I'm working on a project called Acme and the repo is at github.com/me/acme"

Homun stores these in your user profile (`~/.homun/brain/USER.md`). You can view and edit memories in the **Memory** page of the Web UI.

---

## What's next?

Now that Homun is running, here are things to explore:

| Feature | Where | What it does |
|---------|-------|-------------|
| **Knowledge base** | Web UI > Knowledge | Upload docs (PDF, DOCX, code) and ask questions about them |
| **Skills** | Web UI > Skills | Install community skills from GitHub or ClawHub |
| **MCP servers** | Web UI > MCP | Connect external services (Google Workspace, GitHub, etc.) |
| **Browser automation** | Chat | Ask Homun to browse the web ("Go to example.com and...") |
| **Vault** | Web UI > Vault | Store secrets securely (API keys, passwords) with 2FA |
| **Workflows** | Web UI > Workflows | Multi-step tasks with approval gates |
| **OS service** | CLI | `homun service install` to run Homun on boot |

---

## Useful commands

```bash
homun status           # System health check
homun skills list      # List installed skills
homun skills add owner/repo  # Install a skill from GitHub
homun cron list        # List scheduled jobs
homun service install  # Install as OS service (launchd/systemd)
homun stop             # Stop the running gateway
```

---

## Troubleshooting

**Can't access https://localhost**: accept the self-signed certificate warning in your browser, or set `HOMUN_DOMAIN` in `.env` for a real domain with automatic HTTPS.

**LLM provider errors**: check your API key in the web Settings page. Use `homun status` to see provider health.

**Docker not starting**: check logs with `docker compose logs homun`. Ensure port 443 is not in use.

**Ollama models not working**: ensure Ollama is running (`ollama serve`) and you've pulled a model (`ollama pull qwen3:latest`).

For more details, see the [architecture docs](services/README.md) or check the [changelog](../CHANGELOG.md).
