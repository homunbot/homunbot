const icons = {
  menu: `
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 7h16"></path>
      <path d="M4 12h16"></path>
      <path d="M4 17h11"></path>
    </svg>
  `,
  compose: `
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 20h4l10-10-4-4L4 16v4z"></path>
      <path d="M12.5 7.5l4 4"></path>
    </svg>
  `,
  plus: `
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M12 5v14"></path>
      <path d="M5 12h14"></path>
    </svg>
  `,
  send: `
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M5 12h12"></path>
      <path d="M13 8l4 4-4 4"></path>
    </svg>
  `,
  stop: `
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <rect x="7" y="7" width="10" height="10" rx="2"></rect>
    </svg>
  `,
  close: `
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M7 7l10 10"></path>
      <path d="M17 7L7 17"></path>
    </svg>
  `,
  chevron: `
    <svg class="chevron" viewBox="0 0 20 20" aria-hidden="true">
      <path d="M7 4l6 6-6 6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path>
    </svg>
  `,
  sparkle: `
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M12 4l1.6 4.4L18 10l-4.4 1.6L12 16l-1.6-4.4L6 10l4.4-1.6L12 4z"></path>
    </svg>
  `,
};

const demoData = {
  sections: [
    { id: "dashboard", label: "Dashboard", view: "dashboard", tab: "overview" },
    { id: "workflows", label: "Workflows", view: "dashboard", tab: "workflows" },
    { id: "memory", label: "Memory", view: "dashboard", tab: "memory" },
    { id: "mcp", label: "MCP", view: "dashboard", tab: "mcp" },
    { id: "vault", label: "Vault", view: "vault" },
  ],
  suggestions: [
    {
      title: "Prepara il recap della giornata",
      body: "Unisci Memory, task recenti e note sparse in una risposta breve e operativa.",
    },
    {
      title: "Crea un workflow per il follow-up",
      body: "Definisci step, approvazioni e consegna finale senza uscire dalla chat.",
    },
    {
      title: "Raccogli il contesto dal vault leggero",
      body: "Recupera file e servizi MCP giusti solo quando servono davvero.",
    },
  ],
  conversations: [
    {
      id: "new-chat",
      title: "Nuova chat",
      preview: "Composer pulito e contesto allegato quando serve.",
      time: "Ora",
      group: "Oggi",
    },
    {
      id: "weekly-review",
      title: "Review settimanale",
      preview: "Memory, workflow e recap del team marketing.",
      time: "09:24",
      group: "Oggi",
    },
    {
      id: "sales-ops",
      title: "Workflow clienti in prova",
      preview: "Bozza approvazione per follow-up commerciale.",
      time: "Ieri",
      group: "Ieri",
    },
    {
      id: "product-sync",
      title: "Sync prodotto Q2",
      preview: "Roadmap, blocchi e messaggi da inviare al team.",
      time: "Mar 08",
      group: "Meno recenti",
    },
    {
      id: "automation-ideas",
      title: "Idee automazione",
      preview: "Digest mattutino, memoria viva e report canali.",
      time: "Mar 05",
      group: "Meno recenti",
    },
  ],
  attachments: [
    { id: "photo", label: "foto_boardwalk.jpg", meta: "IMG - Camera", type: "attachment" },
    { id: "brief", label: "brief_settimanale.pdf", meta: "PDF - 1.2 MB", type: "attachment" },
    { id: "outline", label: "outline_workflow.md", meta: "MD - 4 note", type: "attachment" },
  ],
  mcpServers: [
    { id: "calendar", title: "Calendar MCP", meta: "Agenda e disponibilita del team", badge: "Attivo" },
    { id: "drive", title: "Drive MCP", meta: "Cartelle e documenti condivisi", badge: "Connesso" },
    { id: "crm", title: "CRM MCP", meta: "Contatti e follow-up commerciali", badge: "Beta" },
  ],
  planSteps: [
    "Raccolgo contesto dalla Memory personale",
    "Compongo il workflow della review",
    "Preparo il recap finale con priorita e handoff",
    "Archiviazione finale e link condivisibili",
  ],
  toolSteps: [
    {
      name: "Memory",
      detail: "Recupero note da recap e automazioni attive",
      done: "Contesto rilevante pronto",
      running: "Scorro gli ultimi blocchi utili",
    },
    {
      name: "Workflow",
      detail: "Costruisco step e checkpoint per la review",
      done: "Bozza workflow pronta",
      running: "Compongo approvazioni e output",
    },
    {
      name: "MCP",
      detail: "Collego solo i servizi selezionati dal composer",
      done: "Calendar MCP collegato",
      running: "In coda finche non serve",
    },
  ],
  workflow: {
    name: "Review settimanale",
    running: {
      copy: "Step 3 di 5 - consolidamento decisioni e prossime azioni.",
      progress: 0.6,
      value: "3/5",
      color: "var(--accent)",
      status: "running",
    },
    paused: {
      copy: "In attesa di approvazione sul recap prima dell'invio ai referenti.",
      progress: 0.5,
      value: "Pausa",
      color: "var(--warn)",
      status: "paused",
    },
    completed: {
      copy: "Pronto, consegnato e salvato in Memory con tutti i riferimenti utili.",
      progress: 1,
      value: "OK",
      color: "var(--ok)",
      status: "completed",
    },
  },
  thinking: [
    "Sto leggendo il contesto utile gia presente in Memory per evitare ripetizioni.",
    "Compongo un workflow breve con un solo punto di approvazione prima dell'invio finale.",
    "Tengo tool activity e stato run secondari, visibili solo quando servono."
  ],
  approval: {
    title: "Approvazione richiesta",
    summary: "Invio recap ai referenti marketing",
    command: "Confermo invio del recap con prossime azioni e link ai documenti condivisi.",
    meta: "Workflow step 4 di 5 - canale interno"
  },
  notices: {
    offline: "Connessione assente. Il thread resta leggibile ma la run non puo continuare finche non torni online.",
    error: "La run si e fermata per un errore recuperabile. Puoi riprovare oppure ripartire dall'ultimo step valido.",
    completed: "Output consolidato e salvato in Memory. Il recap resta pronto per condivisione o riuso."
  },
  dashboard: {
    metrics: [
      { label: "Run attive", value: "04", meta: "2 con approval gate" },
      { label: "Workflow oggi", value: "11", meta: "8 completati" },
      { label: "Memorie utili", value: "26", meta: "7 aggiornate oggi" },
    ],
    workflows: [
      { title: "Review settimanale", meta: "In attesa approvazione", tone: "warn" },
      { title: "Follow-up clienti trial", meta: "2 step rimanenti", tone: "accent" },
      { title: "Digest founder update", meta: "Consegnato e salvato", tone: "ok" },
    ],
    memory: [
      { title: "Preferenze recap", meta: "Tono breve, operativo, con handoff chiari" },
      { title: "Contesto marketing", meta: "Q2, launch pages, tracking e follow-up" },
      { title: "Ultimo summary utile", meta: "Consolidato oggi alle 09:24" },
    ],
    mcp: [
      { title: "Calendar MCP", meta: "Connesso e pronto per disponibilita e slot" },
      { title: "Drive MCP", meta: "Index leggero dei documenti condivisi" },
      { title: "CRM MCP", meta: "Beta, usato solo sui workflow commerciali" },
    ],
  },
  vault: {
    pairing: {
      title: "Pairing attivo",
      meta: "iPhone di Fabio - canale cifrato trusted",
      copy: "Il vault mobile puo mostrare secret e token senza OTP aggiuntive perche il pairing e gia cifrato end-to-end."
    },
    secrets: [
      { title: "OpenAI API Key", meta: "Ultima rotazione 3 giorni fa", status: "Disponibile" },
      { title: "GitHub PAT", meta: "Scope repo + workflow", status: "Pronto" },
      { title: "Google OAuth refresh", meta: "Usato da Calendar MCP", status: "Attivo" },
    ],
    notifications: [
      { title: "Approval push", meta: "Approve / deny direttamente dalla notifica" },
      { title: "Workflow alert", meta: "Step fallito o in attesa intervento" },
    ],
  },
};

const state = {
  screen: "welcome",
  drawerState: "closed",
  dashboardTab: "overview",
  runState: "idle",
  connectionState: "live",
  workflowState: "hidden",
  approvalState: "hidden",
  composerSheet: "closed",
  selectedConversationId: "weekly-review",
  drawerSearch: "",
  composerText: "",
  pendingAttachmentIds: [],
  selectedMcpIds: [],
  planExpanded: false,
  toolExpanded: false,
  thinkingExpanded: false,
};

const appRoot = document.getElementById("app-root");
const demoControls = document.getElementById("demo-controls");

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function currentConversation() {
  return demoData.conversations.find((conversation) => conversation.id === state.selectedConversationId)
    || demoData.conversations[0];
}

function activeWorkflow() {
  if (state.workflowState === "running") return demoData.workflow.running;
  if (state.workflowState === "paused") return demoData.workflow.paused;
  if (state.workflowState === "completed") return demoData.workflow.completed;
  return null;
}

function currentPlanItems() {
  if (state.runState === "working") {
    return [
      { label: demoData.planSteps[0], status: "completed" },
      { label: demoData.planSteps[1], status: "completed" },
      { label: demoData.planSteps[2], status: "in-progress" },
      { label: demoData.planSteps[3], status: "pending" },
    ];
  }

  if (state.runState === "stopping") {
    return [
      { label: demoData.planSteps[0], status: "completed" },
      { label: demoData.planSteps[1], status: "completed" },
      { label: demoData.planSteps[2], status: "in-progress" },
      { label: demoData.planSteps[3], status: "pending" },
    ];
  }

  if (state.workflowState === "completed") {
    return demoData.planSteps.map((label) => ({ label, status: "completed" }));
  }

  return [
    { label: demoData.planSteps[0], status: "completed" },
    { label: demoData.planSteps[1], status: "completed" },
    { label: demoData.planSteps[2], status: "completed" },
    { label: demoData.planSteps[3], status: "pending" },
  ];
}

function planSummary(items) {
  const completed = items.filter((item) => item.status === "completed").length;
  return `${completed} di ${items.length} attivita completate`;
}

function currentToolItems() {
  if (state.runState === "working") {
    return [
      { ...demoData.toolSteps[0], status: "done", note: demoData.toolSteps[0].done },
      { ...demoData.toolSteps[1], status: "running", note: demoData.toolSteps[1].running },
      { ...demoData.toolSteps[2], status: "queued", note: demoData.toolSteps[2].running },
    ];
  }

  if (state.runState === "stopping") {
    return [
      { ...demoData.toolSteps[0], status: "done", note: demoData.toolSteps[0].done },
      { ...demoData.toolSteps[1], status: "running", note: "Sto chiudendo lo step corrente senza crearne altri" },
      { ...demoData.toolSteps[2], status: "queued", note: demoData.toolSteps[2].running },
    ];
  }

  if (state.workflowState === "completed") {
    return demoData.toolSteps.map((tool) => ({
      ...tool,
      status: "done",
      note: tool.done,
    }));
  }

  return [
    { ...demoData.toolSteps[0], status: "done", note: demoData.toolSteps[0].done },
    { ...demoData.toolSteps[1], status: "done", note: demoData.toolSteps[1].done },
    { ...demoData.toolSteps[2], status: "done", note: demoData.toolSteps[2].done },
  ];
}

function statusLabel() {
  if (state.connectionState === "offline") return "Offline";
  if (state.connectionState === "error") return "Errore";
  if (state.runState === "working") return "In esecuzione";
  if (state.runState === "stopping") return "Arresto";
  return "Pronto";
}

function metaLabel() {
  if (state.connectionState === "offline") return "Riconnessione necessaria";
  if (state.connectionState === "error") return "Run interrotta";
  if (state.approvalState === "pending") return "In attesa di approvazione";
  if (state.runState === "working") return "Memory + Workflow";
  if (state.runState === "stopping") return "Chiudo la run attuale";
  if (state.workflowState === "completed") return "Output consolidato";
  return "Chat privata";
}

function assistantCopy() {
  if (state.runState === "working") {
    return [
      "Sto intrecciando Memory e workflow in un thread unico, con piano, stato e consegna sempre leggibili anche su mobile.",
    ];
  }

  if (state.runState === "stopping") {
    return [
      "La run e in arresto controllato: conservo il contesto gia raccolto e fermo gli step non indispensabili senza perdere chiarezza nel thread.",
    ];
  }

  if (state.workflowState === "completed") {
    return [
      "Review completata e archiviata in forma leggibile: il thread resta minimale ma mostra con precisione cosa e stato usato e cosa e successo.",
    ];
  }

  if (state.connectionState === "error") {
    return [
      "Ho mantenuto il contesto gia valido e fermato solo il passaggio che ha generato errore, cosi il thread resta recuperabile senza rumore."
    ];
  }

  return [
    "Homun parte da una conversazione semplice e fa emergere strumenti, workflow e memoria solo quando il task lo richiede davvero.",
  ];
}

function selectedAttachments() {
  return demoData.attachments.filter((attachment) => state.pendingAttachmentIds.includes(attachment.id));
}

function selectedMcpServers() {
  return demoData.mcpServers.filter((server) => state.selectedMcpIds.includes(server.id));
}

function groupedConversations() {
  const query = state.drawerSearch.trim().toLowerCase();
  const groups = new Map();

  demoData.conversations.forEach((conversation) => {
    const matches = !query
      || conversation.title.toLowerCase().includes(query)
      || conversation.preview.toLowerCase().includes(query);
    if (!matches) return;
    if (!groups.has(conversation.group)) {
      groups.set(conversation.group, []);
    }
    groups.get(conversation.group).push(conversation);
  });

  return Array.from(groups.entries());
}

function filteredSections() {
  const query = state.drawerSearch.trim().toLowerCase();
  if (!query) return demoData.sections;
  return demoData.sections.filter((section) => section.label.toLowerCase().includes(query));
}

function approvalLabel() {
  if (state.approvalState === "approved") return "Approvato";
  if (state.approvalState === "always") return "Sempre approvato";
  if (state.approvalState === "denied") return "Negato";
  return "In attesa";
}

function renderInlineNotice() {
  if (state.connectionState === "offline") {
    return `
      <section class="inline-notice" data-tone="offline">
        <div class="inline-notice__title">Offline</div>
        <p>${escapeHtml(demoData.notices.offline)}</p>
      </section>
    `;
  }

  if (state.connectionState === "error") {
    return `
      <section class="inline-notice" data-tone="error">
        <div class="inline-notice__title">Run interrotta</div>
        <p>${escapeHtml(demoData.notices.error)}</p>
      </section>
    `;
  }

  if (state.workflowState === "completed") {
    return `
      <section class="inline-notice" data-tone="ok">
        <div class="inline-notice__title">Memory aggiornata</div>
        <p>${escapeHtml(demoData.notices.completed)}</p>
      </section>
    `;
  }

  return "";
}

function renderThinkingCard() {
  if (state.runState !== "working") return "";

  return `
    <section class="thinking-card" data-expanded="${String(state.thinkingExpanded)}">
      <button class="thinking-card__header" type="button" data-action="toggle-thinking">
        <span class="thinking-card__meta">
          <span class="thinking-card__title">Thinking</span>
          <span class="thinking-card__summary">Ragionamento compatto e collassabile</span>
        </span>
        ${icons.chevron}
      </button>
      <div class="thinking-card__body">
        ${demoData.thinking.map((item) => `<p>${escapeHtml(item)}</p>`).join("")}
      </div>
    </section>
  `;
}

function renderApprovalCard() {
  if (state.approvalState === "hidden") return "";

  if (state.approvalState !== "pending") {
    return `
      <section class="approval-card approval-card--resolved" data-state="${escapeHtml(state.approvalState)}">
        <div class="approval-card__eyebrow">Approvazione</div>
        <div class="approval-card__resolved">${escapeHtml(approvalLabel())}</div>
        <p>${escapeHtml(demoData.approval.summary)}</p>
      </section>
    `;
  }

  return `
    <section class="approval-card" data-state="pending">
      <div class="approval-card__eyebrow">${escapeHtml(demoData.approval.title)}</div>
      <div class="approval-card__title">${escapeHtml(demoData.approval.summary)}</div>
      <p class="approval-card__command">${escapeHtml(demoData.approval.command)}</p>
      <div class="approval-card__meta">${escapeHtml(demoData.approval.meta)}</div>
      <div class="approval-card__actions">
        <button class="approval-action" type="button" data-action="approval-deny">Nega</button>
        <button class="approval-action" type="button" data-action="approval-always">Sempre</button>
        <button class="approval-action approval-action--primary" type="button" data-action="approval-approve">Approva</button>
      </div>
    </section>
  `;
}

function renderControlGroup(label, key, options, columns = 2) {
  const gridClass = columns === 3 ? "control-grid control-grid--three" : "control-grid";
  return `
    <section class="control-group">
      <div class="control-group__label">${escapeHtml(label)}</div>
      <div class="${gridClass}">
        ${options.map((option) => `
          <button
            class="control-button"
            type="button"
            data-control="${escapeHtml(key)}"
            data-value="${escapeHtml(option.value)}"
            aria-pressed="${String(state[key] === option.value)}"
          >
            ${escapeHtml(option.label)}
          </button>
        `).join("")}
      </div>
    </section>
  `;
}

function renderControls() {
  demoControls.innerHTML = `
    ${renderControlGroup("Schermata", "screen", [
      { value: "welcome", label: "Welcome" },
      { value: "chat", label: "Chat attiva" },
      { value: "dashboard", label: "Dashboard" },
      { value: "vault", label: "Vault" },
    ])}

    ${renderControlGroup("Drawer", "drawerState", [
      { value: "closed", label: "Closed" },
      { value: "open", label: "Open" },
    ], 2)}

    ${renderControlGroup("Run", "runState", [
      { value: "idle", label: "Idle" },
      { value: "working", label: "Working" },
      { value: "stopping", label: "Stopping" },
    ], 3)}

    ${renderControlGroup("Connessione", "connectionState", [
      { value: "live", label: "Live" },
      { value: "offline", label: "Offline" },
      { value: "error", label: "Errore" },
    ], 3)}

    ${renderControlGroup("Workflow", "workflowState", [
      { value: "hidden", label: "Nascosto" },
      { value: "running", label: "Running" },
      { value: "paused", label: "Paused" },
      { value: "completed", label: "Completed" },
    ])}

    ${renderControlGroup("Approval", "approvalState", [
      { value: "hidden", label: "Nessuna" },
      { value: "pending", label: "Pending" },
      { value: "approved", label: "Approved" },
      { value: "denied", label: "Denied" },
    ])}

    ${renderControlGroup("Sheet", "composerSheet", [
      { value: "closed", label: "Closed" },
      { value: "plus", label: "Plus" },
      { value: "mcp", label: "MCP" },
    ], 3)}

    <button class="control-action" type="button" data-action="reset-demo">Reset demo</button>
    <p class="control-note">
      Le interazioni interne al mockup aggiornano gli stessi stati del pannello: drawer, sheet, chips, run e workflow.
    </p>
  `;
}

function renderSuggestions() {
  return `
    <section class="hero-card">
      <span class="hero-eyebrow">Terracotta mobile preview</span>

      <div class="hero-copy">
        <h2>Una chat chiara, operativa e profondamente Homun.</h2>
        <p>
          Il canvas parte leggero: conversazione, contesto e una sola azione dominante.
          Workflow, Memory e MCP entrano solo quando servono.
        </p>
      </div>

      <div class="suggestion-list">
        ${demoData.suggestions.map((suggestion, index) => `
          <button class="suggestion-card" type="button" data-action="suggestion" data-index="${index}">
            <span class="suggestion-card__body">
              <strong>${escapeHtml(suggestion.title)}</strong>
              <span>${escapeHtml(suggestion.body)}</span>
            </span>
          </button>
        `).join("")}
      </div>
    </section>
  `;
}

function activeDashboardItems() {
  if (state.dashboardTab === "workflows") return demoData.dashboard.workflows;
  if (state.dashboardTab === "memory") return demoData.dashboard.memory;
  if (state.dashboardTab === "mcp") return demoData.dashboard.mcp;
  return demoData.dashboard.workflows;
}

function activeDashboardLabel() {
  if (state.dashboardTab === "memory") return "Memory";
  if (state.dashboardTab === "mcp") return "MCP";
  if (state.dashboardTab === "workflows") return "Workflows";
  return "Overview";
}

function renderDashboardScreen() {
  const items = activeDashboardItems();
  return `
    <section class="dashboard-stack">
      <div class="thread-intro">
        <div class="thread-intro__title">Dashboard</div>
        <div class="thread-intro__meta">
          <span class="meta-chip">Mobile summary</span>
          <span class="meta-chip">${escapeHtml(activeDashboardLabel())}</span>
        </div>
      </div>

      <section class="dashboard-metrics">
        ${demoData.dashboard.metrics.map((metric) => `
          <article class="dashboard-metric">
            <span class="dashboard-metric__label">${escapeHtml(metric.label)}</span>
            <strong class="dashboard-metric__value">${escapeHtml(metric.value)}</strong>
            <span class="dashboard-metric__meta">${escapeHtml(metric.meta)}</span>
          </article>
        `).join("")}
      </section>

      <section class="dashboard-panel">
        <div class="dashboard-panel__header">
          <span class="dashboard-panel__title">${escapeHtml(activeDashboardLabel())}</span>
          <div class="dashboard-tabs">
            <button class="dashboard-tab ${state.dashboardTab === "overview" ? "is-active" : ""}" type="button" data-action="set-dashboard-tab" data-tab="overview">Overview</button>
            <button class="dashboard-tab ${state.dashboardTab === "workflows" ? "is-active" : ""}" type="button" data-action="set-dashboard-tab" data-tab="workflows">Workflows</button>
            <button class="dashboard-tab ${state.dashboardTab === "memory" ? "is-active" : ""}" type="button" data-action="set-dashboard-tab" data-tab="memory">Memory</button>
            <button class="dashboard-tab ${state.dashboardTab === "mcp" ? "is-active" : ""}" type="button" data-action="set-dashboard-tab" data-tab="mcp">MCP</button>
          </div>
        </div>
        <div class="dashboard-list">
          ${items.map((item) => `
            <article class="dashboard-row" data-tone="${escapeHtml(item.tone || "neutral")}">
              <div class="dashboard-row__title">${escapeHtml(item.title)}</div>
              <div class="dashboard-row__meta">${escapeHtml(item.meta)}</div>
            </article>
          `).join("")}
        </div>
      </section>
    </section>
  `;
}

function renderVaultScreen() {
  return `
    <section class="vault-stack">
      <div class="thread-intro">
        <div class="thread-intro__title">Vault</div>
        <div class="thread-intro__meta">
          <span class="status-chip" data-state="idle">Trusted</span>
          <span class="meta-chip">Pairing cifrato</span>
        </div>
      </div>

      <section class="vault-panel vault-panel--pairing">
        <div class="vault-panel__eyebrow">${escapeHtml(demoData.vault.pairing.title)}</div>
        <div class="vault-panel__title">${escapeHtml(demoData.vault.pairing.meta)}</div>
        <p>${escapeHtml(demoData.vault.pairing.copy)}</p>
      </section>

      <section class="vault-panel">
        <div class="vault-panel__eyebrow">Secret pronti</div>
        <div class="vault-list">
          ${demoData.vault.secrets.map((item) => `
            <article class="vault-row">
              <div class="vault-row__title">${escapeHtml(item.title)}</div>
              <div class="vault-row__meta">${escapeHtml(item.meta)}</div>
              <div class="vault-row__status">${escapeHtml(item.status)}</div>
            </article>
          `).join("")}
        </div>
      </section>

      <section class="vault-panel">
        <div class="vault-panel__eyebrow">Push e approval</div>
        <div class="vault-list">
          ${demoData.vault.notifications.map((item) => `
            <article class="vault-row">
              <div class="vault-row__title">${escapeHtml(item.title)}</div>
              <div class="vault-row__meta">${escapeHtml(item.meta)}</div>
            </article>
          `).join("")}
        </div>
      </section>
    </section>
  `;
}

function renderPlanCard() {
  const items = currentPlanItems();
  return `
    <section class="plan-card" data-expanded="${String(state.planExpanded)}">
      <button class="plan-card__header" type="button" data-action="toggle-plan">
        <span class="plan-card__meta">
          <span class="plan-card__title">Piano attivo</span>
          <span class="plan-card__summary">${escapeHtml(planSummary(items))}</span>
        </span>
        ${icons.chevron}
      </button>
      <div class="plan-list">
        ${items.map((item, index) => `
          <div class="plan-item" data-status="${item.status}">
            <span class="plan-item__marker">${item.status === "completed" ? "1" : index + 1}</span>
            <span>${escapeHtml(item.label)}</span>
          </div>
        `).join("")}
      </div>
    </section>
  `;
}

function renderToolCard() {
  const items = currentToolItems();
  const doneCount = items.filter((item) => item.status === "done").length;
  const summary = state.runState === "working"
    ? `${doneCount} step chiusi, 1 in corso`
    : state.runState === "stopping"
      ? `${doneCount} step chiusi, arresto morbido`
      : `${doneCount} step completati`;

  return `
    <section class="tool-card" data-expanded="${String(state.toolExpanded)}">
      <button class="tool-card__header" type="button" data-action="toggle-tools">
        <span class="tool-card__meta">
          <span class="tool-card__title">Attivita strumenti</span>
          <span class="tool-card__summary">${escapeHtml(summary)}</span>
        </span>
        ${icons.chevron}
      </button>
      <div class="tool-list">
        ${items.map((item) => `
          <div class="tool-item">
            <span class="tool-item__marker"></span>
            <div class="tool-item__body">
              <span class="tool-item__name">${escapeHtml(item.name)}</span>
              <span class="tool-item__detail">${escapeHtml(item.note)}</span>
            </div>
            <span class="tool-item__state" data-status="${item.status}">
              ${escapeHtml(item.status === "done" ? "Done" : item.status === "running" ? "Running" : "Queued")}
            </span>
          </div>
        `).join("")}
      </div>
    </section>
  `;
}

function renderWorkflowCard() {
  const workflow = activeWorkflow();
  if (!workflow) return "";

  return `
    <section class="workflow-card" data-status="${workflow.status}">
      <div
        class="workflow-card__ring"
        style="--ring-progress:${workflow.progress * 360}deg; --ring-color:${workflow.color};"
      >
        <span class="workflow-card__value">${escapeHtml(workflow.value)}</span>
      </div>
      <div class="workflow-card__body">
        <div class="workflow-card__title">${escapeHtml(demoData.workflow.name)}</div>
        <div class="workflow-card__copy">${escapeHtml(workflow.copy)}</div>
      </div>
    </section>
  `;
}

function renderAssistantBlock() {
  const copy = assistantCopy();
  const showPlan = state.runState !== "idle" || state.workflowState === "completed";
  const showTools = state.runState === "working" || state.runState === "stopping" || state.workflowState === "completed";
  const showWorkflow = state.workflowState !== "hidden";
  return `
    <section class="assistant-block">
      <div class="assistant-block__header">
        <img src="../static/img/icon_braun.png" alt="">
        <span>Homun</span>
      </div>
      ${renderInlineNotice()}
      ${renderThinkingCard()}
      ${renderApprovalCard()}
      ${showPlan ? renderPlanCard() : ""}
      ${showTools ? renderToolCard() : ""}
      ${showWorkflow ? renderWorkflowCard() : ""}
      <article class="assistant-message">
        ${copy.map((paragraph) => `<p>${escapeHtml(paragraph)}</p>`).join("")}
        <div class="assistant-message__footer">
          <button class="action-pill" type="button">Rigenera</button>
          <button class="action-pill" type="button">Salva in Memory</button>
        </div>
      </article>
    </section>
  `;
}

function renderChatThread() {
  const conversation = currentConversation();
  return `
    <section class="chat-stack">
      <div class="thread-intro">
        <div class="thread-intro__title">${escapeHtml(conversation.title)}</div>
        <div class="thread-intro__meta">
          <span class="status-chip" data-state="${escapeHtml(state.runState)}">${escapeHtml(statusLabel())}</span>
          <span class="meta-chip">${escapeHtml(metaLabel())}</span>
        </div>
      </div>
      <div class="message-row--user">
        <article class="message-bubble message-bubble--user">
          Preparami un recap mobile-first con piano, tool activity e workflow.
        </article>
      </div>
      ${renderAssistantBlock()}
    </section>
  `;
}

function renderContextChips() {
  const attachmentChips = selectedAttachments().map((attachment) => `
    <div class="context-chip context-chip--asset">
      <span class="context-chip__body">
        <span class="context-chip__label">${escapeHtml(attachment.label)}</span>
        <span class="context-chip__meta">${escapeHtml(attachment.meta)}</span>
      </span>
      <button type="button" data-action="remove-attachment" data-id="${escapeHtml(attachment.id)}" aria-label="Rimuovi allegato">
        ${icons.close}
      </button>
    </div>
  `);

  const mcpChips = selectedMcpServers().map((server) => `
    <div class="context-chip context-chip--asset">
      <span class="context-chip__body">
        <span class="context-chip__label">${escapeHtml(server.title)}</span>
        <span class="context-chip__meta">${escapeHtml(server.badge)} - MCP</span>
      </span>
      <button type="button" data-action="remove-mcp" data-id="${escapeHtml(server.id)}" aria-label="Rimuovi server MCP">
        ${icons.close}
      </button>
    </div>
  `);

  if (attachmentChips.length === 0 && mcpChips.length === 0) return "";

  return `
    <div class="composer-context">
      ${attachmentChips.join("")}
      ${mcpChips.join("")}
    </div>
  `;
}

function renderComposer() {
  const sendIcon = state.runState === "idle" ? icons.send : icons.stop;
  const composerLocked = state.connectionState !== "live";
  const placeholder = composerLocked
    ? "Puoi continuare a scrivere, ma per inviare serve tornare online..."
    : state.screen === "welcome"
      ? "Chiedi a Homun di partire da un obiettivo, un documento o un workflow..."
      : "Continua la chat, collega un servizio o allega contesto...";

  return `
    <div class="composer-zone">
      ${renderContextChips()}
      <section class="composer">
        <textarea id="composer-text" placeholder="${escapeHtml(placeholder)}">${escapeHtml(state.composerText)}</textarea>
        <div class="composer__footer">
          <div class="composer__meta">
            <span class="meta-chip">gpt-5.4</span>
            <span class="meta-chip">${escapeHtml(metaLabel())}</span>
          </div>
          <div class="composer__actions">
            <button class="icon-button" type="button" data-action="open-plus" aria-label="Apri azioni composer">
              ${icons.plus}
            </button>
            <button
              class="icon-button icon-button--primary icon-button--send"
              type="button"
              data-action="toggle-run"
              data-run-state="${escapeHtml(state.runState)}"
              ${composerLocked ? "disabled" : ""}
              aria-label="${state.runState === "idle" ? "Invia messaggio" : "Ferma run"}"
            >
              ${sendIcon}
            </button>
          </div>
        </div>
      </section>
    </div>
  `;
}

function renderTopbar() {
  return `
    <header class="app-topbar">
      <div class="topbar-brand">
        <img src="../static/img/icon_braun.png" alt="Homun">
      </div>

      <div class="topbar-actions">
        <button class="icon-button" type="button" data-action="toggle-drawer" aria-label="Apri cronologia">
          ${icons.menu}
        </button>
        <button class="icon-button icon-button--primary" type="button" data-action="new-chat" aria-label="Nuova chat">
          ${icons.compose}
        </button>
      </div>
    </header>
  `;
}

function renderDrawer() {
  const sections = filteredSections();
  const groups = groupedConversations();
  return `
    <div class="drawer-scrim ${state.drawerState === "open" ? "is-visible" : ""}" data-action="close-drawer"></div>
    <aside class="drawer ${state.drawerState === "open" ? "is-open" : ""}" aria-hidden="${String(state.drawerState !== "open")}">
      <div class="drawer__header">
        <input
          class="drawer__search"
          id="drawer-search"
          type="search"
          value="${escapeHtml(state.drawerSearch)}"
          placeholder="Cerca"
        >

        <div class="drawer-nav">
          ${sections.map((section) => `
            <button
              class="drawer-nav__item ${state.screen === section.view && (!section.tab || state.dashboardTab === section.tab) ? "is-active" : ""}"
              type="button"
              data-action="navigate-section"
              data-view="${escapeHtml(section.view)}"
              ${section.tab ? `data-tab="${escapeHtml(section.tab)}"` : ""}
            >${escapeHtml(section.label)}</button>
          `).join("")}
        </div>
      </div>

      <div class="drawer__list">
        ${sections.length === 0 && groups.length === 0 ? `
          <div class="conversation-item">
            <div class="conversation-item__title">Nessun risultato</div>
          </div>
        ` : groups.map(([group, items]) => `
          <section class="drawer__section">
            <div class="drawer__section-label">${escapeHtml(group)}</div>
            ${items.map((conversation) => `
              <button
                class="conversation-item ${conversation.id === state.selectedConversationId ? "is-active" : ""}"
                type="button"
                data-action="select-conversation"
                data-id="${escapeHtml(conversation.id)}"
              >
                <div class="conversation-item__row">
                  <span class="conversation-item__title">${escapeHtml(conversation.title)}</span>
                  <span class="conversation-item__time">${escapeHtml(conversation.time)}</span>
                </div>
                <span class="conversation-item__preview">${escapeHtml(conversation.preview)}</span>
              </button>
            `).join("")}
          </section>
        `).join("")}
      </div>

      <footer class="drawer__footer">
        <div class="profile-chip">
          <span class="profile-chip__avatar">FB</span>
          <span class="profile-chip__body">
            <span class="profile-chip__name">Fabio</span>
            <span class="profile-chip__role">Owner workspace</span>
          </span>
        </div>
      </footer>
    </aside>
  `;
}

function renderSheet() {
  if (state.composerSheet === "closed") {
    return `<div class="sheet-scrim"></div>`;
  }

  if (state.composerSheet === "plus") {
    return `
      <div class="sheet-scrim is-visible" data-action="close-sheet"></div>
      <section class="sheet is-open" aria-label="Azioni composer">
        <div class="sheet__handle"></div>
        <div class="sheet__header">
          <div class="sheet__title">Aggiungi contesto</div>
          <div class="sheet__copy">Allegati e servizi restano secondari finche non servono davvero.</div>
        </div>
        <div class="sheet__body">
          <button class="sheet-option" type="button" data-action="add-attachment" data-id="photo">
            <span class="sheet-option__copy">
              <span class="sheet-option__title">Aggiungi immagine</span>
              <span class="sheet-option__meta">Camera o galleria come primo contesto mobile</span>
            </span>
            <span class="sheet-option__badge">IMG</span>
          </button>

          <button class="sheet-option" type="button" data-action="add-attachment" data-id="brief">
            <span class="sheet-option__copy">
              <span class="sheet-option__title">Aggiungi documento</span>
              <span class="sheet-option__meta">Inserisce un allegato di esempio nella composer strip</span>
            </span>
            <span class="sheet-option__badge">PDF</span>
          </button>

          <button class="sheet-option" type="button" data-action="open-mcp-sheet">
            <span class="sheet-option__copy">
              <span class="sheet-option__title">Apri MCP</span>
              <span class="sheet-option__meta">Seleziona un server e trasformalo in chip di contesto</span>
            </span>
            <span class="sheet-option__badge">MCP</span>
          </button>

          <button class="sheet-option" type="button" data-action="seed-outline">
            <span class="sheet-option__copy">
              <span class="sheet-option__title">Aggiungi outline workflow</span>
              <span class="sheet-option__meta">Secondo allegato demo per stati piu densi</span>
            </span>
            <span class="sheet-option__badge">MD</span>
          </button>
        </div>
      </section>
    `;
  }

  return `
    <div class="sheet-scrim is-visible" data-action="close-sheet"></div>
    <section class="sheet is-open" aria-label="Server MCP">
      <div class="sheet__handle"></div>
      <div class="sheet__header">
        <div class="sheet__title">Collega un MCP</div>
        <div class="sheet__copy">Il mockup espone i servizi come scelta contestuale, non come rumore costante.</div>
      </div>
      <div class="sheet__body">
        ${demoData.mcpServers.map((server) => `
          <button
            class="sheet-option ${state.selectedMcpIds.includes(server.id) ? "is-selected" : ""}"
            type="button"
            data-action="toggle-mcp"
            data-id="${escapeHtml(server.id)}"
          >
            <span class="sheet-option__copy">
              <span class="sheet-option__title">${escapeHtml(server.title)}</span>
              <span class="sheet-option__meta">${escapeHtml(server.meta)}</span>
            </span>
            <span class="sheet-option__badge">${escapeHtml(server.badge)}</span>
          </button>
        `).join("")}
      </div>
    </section>
  `;
}

function renderApp() {
  const focusMeta = preserveFocus();
  const drawerOpen = state.drawerState === "open";
  const hasComposer = state.screen === "welcome" || state.screen === "chat";
  const content = state.screen === "welcome"
    ? renderSuggestions()
    : state.screen === "chat"
      ? renderChatThread()
      : state.screen === "dashboard"
        ? renderDashboardScreen()
        : renderVaultScreen();

  appRoot.innerHTML = `
    <div class="app-shell">
      <div class="thread-shell" data-drawer-open="${String(drawerOpen)}" data-has-composer="${String(hasComposer)}">
        ${renderTopbar()}

        <div class="thread-layer">
          <div class="thread-scroll">
            ${content}
          </div>

          ${hasComposer ? renderComposer() : ""}
        </div>

        ${renderDrawer()}
        ${hasComposer ? renderSheet() : `<div class="sheet-scrim"></div>`}
      </div>
    </div>
  `;

  restoreFocus(focusMeta);
}

function seedContextIfNeeded() {
  if (state.selectedMcpIds.length === 0) {
    state.selectedMcpIds = ["calendar"];
  }
}

function preserveFocus() {
  const active = document.activeElement;
  if (!active) return null;

  if (active.id === "drawer-search" || active.id === "composer-text") {
    return {
      id: active.id,
      start: active.selectionStart,
      end: active.selectionEnd,
    };
  }

  return null;
}

function restoreFocus(meta) {
  if (!meta) return;
  const next = document.getElementById(meta.id);
  if (!next) return;
  next.focus();
  if (typeof meta.start === "number" && typeof meta.end === "number" && typeof next.setSelectionRange === "function") {
    next.setSelectionRange(meta.start, meta.end);
  }
}

function resetDemo() {
  state.screen = "welcome";
  state.drawerState = "closed";
  state.dashboardTab = "overview";
  state.runState = "idle";
  state.connectionState = "live";
  state.workflowState = "hidden";
  state.approvalState = "hidden";
  state.composerSheet = "closed";
  state.selectedConversationId = "weekly-review";
  state.drawerSearch = "";
  state.composerText = "";
  state.pendingAttachmentIds = [];
  state.selectedMcpIds = [];
  state.planExpanded = false;
  state.toolExpanded = false;
  state.thinkingExpanded = false;
}

function setControl(key, value) {
  state[key] = value;

  if (key === "screen") {
    state.drawerState = "closed";
    if (value !== "chat" && value !== "welcome") {
      state.composerSheet = "closed";
    }
  }

  if (key === "screen" && value === "welcome") {
    state.composerSheet = "closed";
    state.pendingAttachmentIds = [];
    state.selectedMcpIds = [];
  }

  if (key === "screen" && value === "chat") {
    seedContextIfNeeded();
  }

  if (key === "runState" && value === "idle" && state.workflowState === "hidden") {
    state.planExpanded = false;
    state.toolExpanded = false;
  }

  if (key === "runState" && value === "working" && state.connectionState !== "live") {
    state.connectionState = "live";
  }

  if (key === "workflowState" && value === "paused" && state.approvalState === "hidden") {
    state.approvalState = "pending";
  }

  if (key === "workflowState" && value === "running" && state.approvalState === "pending") {
    state.approvalState = "hidden";
  }

  if (key === "approvalState" && value === "pending") {
    state.workflowState = "paused";
    state.runState = "idle";
  }

  if (key === "approvalState" && value !== "pending" && state.workflowState === "paused") {
    state.runState = "idle";
  }

  if (key === "connectionState" && value !== "live") {
    state.runState = "idle";
  }

  renderControls();
  renderApp();
}

function closeSheet() {
  state.composerSheet = "closed";
}

function selectConversation(conversationId) {
  state.selectedConversationId = conversationId;
  seedContextIfNeeded();
  state.screen = "chat";
  state.drawerState = "closed";
  state.composerSheet = "closed";
}

function startNewChat() {
  state.screen = "welcome";
  state.drawerState = "closed";
  state.dashboardTab = "overview";
  state.runState = "idle";
  state.connectionState = "live";
  state.workflowState = "hidden";
  state.approvalState = "hidden";
  state.composerSheet = "closed";
  state.selectedConversationId = "new-chat";
  state.composerText = "";
  state.pendingAttachmentIds = [];
  state.selectedMcpIds = [];
  state.planExpanded = false;
  state.toolExpanded = false;
  state.thinkingExpanded = false;
}

function toggleRun() {
  if (state.connectionState !== "live") return;

  if (state.screen === "welcome") {
    state.screen = "chat";
    state.drawerState = "closed";
    state.selectedConversationId = "weekly-review";
    seedContextIfNeeded();
  }

  if (state.runState === "idle") {
    state.runState = "working";
    state.approvalState = "hidden";
    if (state.workflowState === "hidden") {
      state.workflowState = "running";
    }
  } else if (state.runState === "working") {
    state.runState = "stopping";
  } else {
    state.runState = "idle";
    if (state.workflowState === "running") {
      state.workflowState = "paused";
      state.approvalState = "pending";
    }
  }
}

function handleAction(action, target) {
  if (action === "toggle-drawer") {
    state.drawerState = state.drawerState === "open" ? "closed" : "open";
  } else if (action === "close-drawer") {
    state.drawerState = "closed";
  } else if (action === "new-chat") {
    startNewChat();
  } else if (action === "navigate-section") {
    state.screen = target.dataset.view || "chat";
    state.drawerState = "closed";
    state.composerSheet = "closed";
    if (target.dataset.tab) {
      state.dashboardTab = target.dataset.tab;
    } else if (state.screen === "dashboard") {
      state.dashboardTab = "overview";
    }
  } else if (action === "toggle-plan") {
    state.planExpanded = !state.planExpanded;
  } else if (action === "toggle-tools") {
    state.toolExpanded = !state.toolExpanded;
  } else if (action === "toggle-thinking") {
    state.thinkingExpanded = !state.thinkingExpanded;
  } else if (action === "set-dashboard-tab") {
    state.screen = "dashboard";
    state.dashboardTab = target.dataset.tab || "overview";
    state.drawerState = "closed";
  } else if (action === "open-plus") {
    state.composerSheet = state.composerSheet === "plus" ? "closed" : "plus";
  } else if (action === "open-mcp-sheet") {
    state.composerSheet = "mcp";
  } else if (action === "close-sheet") {
    closeSheet();
  } else if (action === "toggle-run") {
    toggleRun();
  } else if (action === "add-attachment") {
    const attachmentId = target.dataset.id;
    if (!state.pendingAttachmentIds.includes(attachmentId)) {
      state.pendingAttachmentIds.push(attachmentId);
    }
    closeSheet();
  } else if (action === "seed-outline") {
    if (!state.pendingAttachmentIds.includes("outline")) {
      state.pendingAttachmentIds.push("outline");
    }
    closeSheet();
  } else if (action === "remove-attachment") {
    const attachmentId = target.dataset.id;
    state.pendingAttachmentIds = state.pendingAttachmentIds.filter((id) => id !== attachmentId);
  } else if (action === "toggle-mcp") {
    const serverId = target.dataset.id;
    if (state.selectedMcpIds.includes(serverId)) {
      state.selectedMcpIds = state.selectedMcpIds.filter((id) => id !== serverId);
    } else {
      state.selectedMcpIds = [...state.selectedMcpIds, serverId];
    }
  } else if (action === "remove-mcp") {
    const serverId = target.dataset.id;
    state.selectedMcpIds = state.selectedMcpIds.filter((id) => id !== serverId);
  } else if (action === "select-conversation") {
    selectConversation(target.dataset.id);
  } else if (action === "approval-approve") {
    state.approvalState = "approved";
    state.workflowState = "running";
    state.runState = "working";
  } else if (action === "approval-always") {
    state.approvalState = "always";
    state.workflowState = "running";
    state.runState = "working";
  } else if (action === "approval-deny") {
    state.approvalState = "denied";
    state.workflowState = "paused";
    state.runState = "idle";
  } else if (action === "suggestion") {
    const suggestion = demoData.suggestions[Number(target.dataset.index)];
    state.composerText = suggestion ? suggestion.title : state.composerText;
    state.screen = "chat";
    state.drawerState = "closed";
    state.selectedConversationId = "weekly-review";
    seedContextIfNeeded();
    state.connectionState = "live";
    state.runState = "working";
    state.approvalState = "hidden";
    state.workflowState = "running";
    state.composerSheet = "closed";
  } else if (action === "reset-demo") {
    resetDemo();
  }

  renderControls();
  renderApp();
}

demoControls.addEventListener("click", (event) => {
  const button = event.target.closest("[data-control], [data-action]");
  if (!button) return;

  if (button.dataset.control) {
    setControl(button.dataset.control, button.dataset.value);
    return;
  }

  handleAction(button.dataset.action, button);
});

appRoot.addEventListener("click", (event) => {
  const button = event.target.closest("[data-action]");
  if (!button) return;
  handleAction(button.dataset.action, button);
});

appRoot.addEventListener("input", (event) => {
  if (event.target.id === "drawer-search") {
    state.drawerSearch = event.target.value;
    renderApp();
    return;
  }

  if (event.target.id === "composer-text") {
    state.composerText = event.target.value;
  }
});

resetDemo();
renderControls();
renderApp();
