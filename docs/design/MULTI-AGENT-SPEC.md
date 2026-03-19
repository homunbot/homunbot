# Multi-Agent Architecture Spec

> Status: DRAFT — 2026-03-19
> Prerequisite: LLM-1 (request queue con priorità)
> Scope: Agent registry, pipeline orchestration, task-oriented prompting, enterprise multi-tenant

## Vision

Passare dal modello "singolo agente tuttofare" a un sistema dove **agenti specializzati collaborano** su task complessi. Ogni agente ha il suo provider, il suo contesto, i suoi strumenti — ma condividono la stessa infrastruttura (DB, canali, RAG, vault).

## I 4 Paradigmi

### 1. Task > Ruoli

Oggi il system prompt dice "sei Homun, un assistente". Il nuovo modello:

```toml
# config.toml
[agents.default]
model = "anthropic/claude-sonnet-4-20250514"
instructions = """
Ragiona step-by-step prima di rispondere.
Se il task richiede codice, verifica la correttezza prima di proporlo.
Se non sei sicuro, chiedi chiarimenti invece di inventare.
"""

[agents.coder]
model = "anthropic/claude-sonnet-4-20250514"
instructions = """
Sei specializzato in codice. Usa Chain of Thought.
Scrivi sempre test. Verifica che compili prima di rispondere.
Non spiegare — mostra il codice.
"""
tools = ["shell", "file_read", "file_write"]

[agents.researcher]
model = "openai/gpt-4o"
instructions = """
Cerca informazioni accurate. Cita le fonti.
Usa RAG prima di rispondere. Verifica i fatti.
"""
tools = ["web_search", "web_fetch", "knowledge"]
```

Ogni agente è definito da **istruzioni su come ragionare**, non da un ruolo generico.

### 2. Multi-Agent Pipeline

```
                    ┌─────────────┐
Messaggio utente ──→│   Router    │──→ Agente giusto (o pipeline)
                    └─────────────┘
                          │
              ┌───────────┼───────────┐
              ▼           ▼           ▼
         ┌────────┐  ┌────────┐  ┌────────┐
         │ Coder  │  │Research│  │ Writer │
         │ Agent  │  │ Agent  │  │ Agent  │
         └────┬───┘  └────┬───┘  └────┬───┘
              │           │           │
              └───────────┼───────────┘
                          ▼
                    ┌─────────────┐
                    │  Assembler  │──→ Risposta finale
                    └─────────────┘
```

**Tre pattern di orchestrazione:**

| Pattern | Descrizione | Esempio |
|---------|-------------|---------|
| **Router** | Un agente smista al giusto specialista | "Scrivi un email" → Writer agent |
| **Pipeline** | Output di A diventa input di B | Researcher → Coder → Reviewer |
| **Parallel** | Più agenti lavorano in parallelo, poi merge | Search A + Search B → Merge |

### 3. RAG-First

Già implementato. Ogni agente accede allo stesso RAG engine ma può avere **filtri diversi** (per fonte, per tipo, per contatto). Il knowledge tool è già nel tool registry.

### 4. Few-Shot via Skills

Le skill con esempi concreti nel SKILL.md body sono già few-shot prompting. Ogni agente può avere un **subset di skill** assegnate.

---

## Architettura

### Agent Definition

```rust
/// Un agente con il suo provider, prompt e tool set.
pub struct AgentDefinition {
    /// Unique identifier (e.g. "default", "coder", "researcher")
    pub id: String,
    /// LLM model to use (e.g. "anthropic/claude-sonnet-4-20250514")
    pub model: String,
    /// Task-oriented instructions (replaces role-based "you are...")
    pub instructions: String,
    /// Allowed tools (empty = all tools)
    pub tools: Vec<String>,
    /// Optional skill filter (only these skills are visible)
    pub skills: Vec<String>,
    /// Max concurrent requests for this agent
    pub max_concurrency: u32,
    /// Priority level (affects LLM queue ordering)
    pub priority: Priority,
}
```

### Agent Registry

```rust
/// Pool of available agents, looked up by ID or routing rules.
pub struct AgentRegistry {
    agents: HashMap<String, AgentInstance>,
    router: Box<dyn AgentRouter>,
}

impl AgentRegistry {
    /// Get or create an agent instance by ID.
    fn get(&self, id: &str) -> Option<&AgentInstance>;

    /// Route a message to the best agent based on content/context.
    async fn route(&self, message: &InboundMessage, contact: Option<&Contact>) -> &str;
}
```

### Agent Router

Il router decide quale agente gestisce un messaggio:

```rust
pub trait AgentRouter: Send + Sync {
    /// Decide which agent should handle this message.
    /// Returns agent ID.
    async fn route(
        &self,
        channel: &str,
        content: &str,
        contact: Option<&Contact>,
    ) -> String;
}

/// Simple router: uses channel/contact config to pick agent.
pub struct ConfigRouter;

/// LLM router: asks a fast model to classify the task.
pub struct LlmRouter {
    classifier_model: String,
}
```

**Routing rules (priority order):**
1. Contact ha `agent_override` → usa quello
2. Channel ha `default_agent` → usa quello
3. LLM classifier (opzionale) → analizza il messaggio e sceglie
4. Fallback → agente "default"

### Pipeline Orchestration

Estende il workflow engine esistente (`src/workflows/`):

```rust
pub struct AgentPipeline {
    steps: Vec<PipelineStep>,
}

pub struct PipelineStep {
    agent_id: String,
    /// Transform output before passing to next step
    transform: Option<String>,  // prompt template
    /// Run in parallel with other steps at same index
    parallel: bool,
}
```

---

## Config Schema

```toml
# Agents definition
[agents.default]
model = "anthropic/claude-sonnet-4-20250514"
max_concurrency = 5

[agents.coder]
model = "anthropic/claude-sonnet-4-20250514"
instructions = "Chain of Thought. Scrivi test. Verifica che compili."
tools = ["shell", "file_read", "file_write"]
max_concurrency = 2

[agents.researcher]
model = "openai/gpt-4o"
instructions = "Cerca info accurate. Cita fonti. Usa RAG."
tools = ["web_search", "web_fetch", "knowledge"]
max_concurrency = 3

[agents.writer]
model = "ollama/llama3"
instructions = "Scrivi in modo chiaro e conciso. Usa italiano formale."
tools = ["message"]
max_concurrency = 1

# Channel → agent binding
[channels.telegram]
default_agent = "default"

[channels.slack]
default_agent = "coder"  # Slack workspace = dev team

# Contact → agent binding (in DB via persona_override or new field)
# Contact "Mario" → agent "researcher" (override)

# Router config
[routing]
mode = "config"  # "config" | "llm" | "hybrid"
classifier_model = "anthropic/claude-haiku"  # fast model for LLM routing
```

---

## Implementation Phases

### Phase 1: Agent Definitions (prerequisite: LLM-1)
1. `AgentDefinition` struct in `config/schema.rs`
2. `[agents.*]` config parsing
3. Each agent gets its own `Provider` instance (via factory)
4. `QueuedProvider` (from LLM-1) manages per-agent concurrency

### Phase 2: Agent Registry + Config Router
1. `AgentRegistry` in `src/agent/registry.rs` (new file, not tools/registry.rs)
2. `ConfigRouter`: channel.default_agent → agent lookup
3. Gateway dispatches to the right agent based on routing
4. Contact `agent_override` field (migration)

### Phase 3: LLM Router
1. `LlmRouter`: fast model classifies incoming message
2. Uses `one_shot.rs` with a classification prompt
3. Caches recent routing decisions per-session

### Phase 4: Pipeline Orchestration
1. Extend workflow engine with `AgentPipeline`
2. Pipeline steps reference agents by ID
3. Output of step N → input of step N+1
4. Parallel execution with merge step

---

## Relationship with Existing Systems

| System | Before | After |
|--------|--------|-------|
| **AgentLoop** | Singleton, shared by all | One per agent definition, pool-managed |
| **Provider** | One per config | One per agent, wrapped in QueuedProvider |
| **ContextBuilder** | One, rebuilds per-message | One per agent, with agent-specific instructions |
| **ToolRegistry** | Global, all tools | Per-agent filtered view |
| **Skills** | Global, all skills | Per-agent skill subset |
| **Sessions** | channel:chat_id | agent:channel:chat_id (agent-scoped) |
| **Memory** | Global + contact-scoped | Agent-scoped + contact-scoped |
| **Subagent (spawn)** | Creates ad-hoc task | Routes to a named agent |

---

## Enterprise Multi-Tenant

Per uso enterprise, ogni "tenant" (cliente/reparto) ha:
- Il suo set di agenti
- Le sue API keys (diversi provider)
- I suoi canali
- Il suo RAG namespace
- Il suo vault

Questo è un layer sopra al multi-agent — richiede `tenant_id` pervasivo nel DB. Fuori scope per ora, ma l'architettura multi-agent lo prepara.

---

## Open Questions

1. **Session isolation**: se un utente parla con "coder" e poi "researcher" sullo stesso canale, condividono la session history? Probabilmente sì — il contesto conversazionale è utile.
2. **Agent handoff**: come gestire "non sono l'agente giusto per questo, passo a X"? Tool `handoff` o routing automatico?
3. **Cost tracking**: ogni agente usa un provider diverso con costi diversi. Tracciare per-agent token usage?
4. **UI**: come mostrare nella Web UI quale agente sta rispondendo? Badge nel chat?
