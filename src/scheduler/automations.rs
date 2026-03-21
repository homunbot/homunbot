use anyhow::{bail, Result};
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use crate::config::Config;
use crate::utils::text::truncate_str;

/// Stored schedule format for automations.
///
/// - `cron:<expr>` for cron expressions
/// - `every:<seconds>` for fixed intervals
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutomationSchedule {
    Cron(String),
    Every(u64),
}

impl AutomationSchedule {
    /// Parse a stored schedule (`cron:...` or `every:...`).
    pub fn parse_stored(schedule: &str) -> Result<Self> {
        if let Some(expr) = schedule.strip_prefix("cron:") {
            Self::from_cron(expr)
        } else if let Some(raw) = schedule.strip_prefix("every:") {
            let secs: u64 = raw
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid every: schedule value"))?;
            Self::from_every(secs)
        } else {
            bail!("Unsupported schedule format: {schedule}");
        }
    }

    /// Build a cron schedule and validate expression syntax.
    pub fn from_cron(expr: &str) -> Result<Self> {
        let normalized = normalize_cron_expr(expr)?;
        // `cron` crate in this setup expects an explicit seconds field.
        // Internally we keep 5-field cron, so we prepend `0` only for validation.
        let validation_expr = format!("0 {normalized}");
        let _parsed: cron::Schedule = validation_expr
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid cron expression: {e}"))?;
        Ok(Self::Cron(normalized))
    }

    /// Build an interval schedule.
    pub fn from_every(seconds: u64) -> Result<Self> {
        if seconds == 0 {
            bail!("Interval must be greater than 0 seconds");
        }
        Ok(Self::Every(seconds))
    }

    /// Serialize to storage string.
    pub fn as_stored(&self) -> String {
        match self {
            Self::Cron(expr) => format!("cron:{expr}"),
            Self::Every(secs) => format!("every:{secs}"),
        }
    }

    /// Compute next run time in UTC, based on current time and optional last_run.
    pub fn next_run_at(&self, now: DateTime<Utc>, last_run: Option<&str>) -> Option<DateTime<Utc>> {
        match self {
            Self::Cron(expr) => {
                // Keep storage in 5-field cron but parse with explicit seconds for the crate.
                let parsed: cron::Schedule = format!("0 {expr}").parse().ok()?;
                parsed.after(&now).next()
            }
            Self::Every(secs) => {
                let secs = (*secs).max(1) as i64;
                let base = parse_last_run(last_run).unwrap_or(now);
                let candidate = base + Duration::seconds(secs);
                if candidate > now {
                    Some(candidate)
                } else {
                    Some(now)
                }
            }
        }
    }

    /// Convenience helper: parse stored schedule and compute next run.
    pub fn next_run_from_stored(
        schedule: &str,
        last_run: Option<&str>,
        now: DateTime<Utc>,
    ) -> Option<DateTime<Utc>> {
        let parsed = Self::parse_stored(schedule).ok()?;
        parsed.next_run_at(now, last_run)
    }
}

/// Normalize an automation prompt for runtime execution.
///
/// If the saved prompt is a "create automation ..." user utterance,
/// extract the actual task instruction so `run now` executes the task
/// instead of trying to create another automation.
pub fn normalize_runtime_prompt(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Some(task) = extract_task_from_structured_summary(trimmed) {
        return task;
    }

    let lower = trimmed.to_ascii_lowercase();
    let likely_creation_request = (lower.contains("crea") && lower.contains("automat"))
        || (lower.contains("create") && lower.contains("automation"))
        || (lower.contains("set up") && lower.contains("automation"))
        || (lower.contains("imposta") && lower.contains("automat"));

    if likely_creation_request {
        if let Some((_, task)) = trimmed.split_once(':') {
            let task = task.trim();
            if !task.is_empty() {
                return task.to_string();
            }
        }

        if let Ok(re) = Regex::new(
            r"(?i)^(?:crea|create|imposta|set up).{0,80}(?:automation|automazione)\b.{0,80}\b(?:che|to)\b\s+(.+)$",
        ) {
            if let Some(c) = re.captures(trimmed) {
                if let Some(task) = c.get(1).map(|m| m.as_str().trim()) {
                    if !task.is_empty() {
                        return task.to_string();
                    }
                }
            }
        }
    }

    trimmed.to_string()
}

fn extract_task_from_structured_summary(raw: &str) -> Option<String> {
    let fields = [
        ("azione", false),
        ("action", false),
        ("prompt", false),
        ("task", true),
    ];

    for line in raw.lines() {
        let mut clean = line.trim().trim_start_matches(|c| {
            matches!(
                c,
                '-' | '*' | '•' | '·' | ':' | '|' | '>' | '[' | ']' | '(' | ')'
            )
        });
        clean = clean.trim_matches('*').trim();
        if clean.is_empty() {
            continue;
        }

        let clean = clean.replace("**", "");
        for (field, cautious) in fields {
            let prefix = format!("{field}:");
            if clean.len() <= prefix.len() {
                continue;
            }
            if clean[..prefix.len()].eq_ignore_ascii_case(&prefix) {
                let candidate = clean[prefix.len()..].trim().trim_matches('`');
                if candidate.is_empty() {
                    continue;
                }
                if !cautious || looks_like_task_instruction(candidate) {
                    return Some(candidate.to_string());
                }
            }
        }
    }

    if let Ok(re) = Regex::new(r"(?i)\b(?:il task|the task)\s+(?:controllera|will)\s+(.+)$") {
        if let Some(c) = re.captures(raw) {
            if let Some(task) = c.get(1).map(|m| m.as_str().trim().trim_end_matches('.')) {
                if looks_like_task_instruction(task) {
                    return Some(task.to_string());
                }
            }
        }
    }

    None
}

fn looks_like_task_instruction(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.split_whitespace().count() < 3 {
        return false;
    }
    let verbs = [
        "controlla",
        "check",
        "leggi",
        "read",
        "cerca",
        "search",
        "invia",
        "send",
        "summarize",
        "riassumi",
        "monitor",
        "analizza",
        "analyze",
    ];
    verbs.iter().any(|v| lower.contains(v))
}

/// Build the user-visible input for an automation run.
///
/// This wraps the normalized task with strict execution-mode guidance so
/// the model executes the task instead of replying with "automation created"
/// confirmations.
pub fn build_runtime_run_input(raw_prompt: &str) -> String {
    build_runtime_run_input_from_plan(None, raw_prompt)
}

/// Resolve task instructions from persisted plan metadata when available.
pub fn runtime_task_from_plan_or_prompt(plan_json: Option<&str>, raw_prompt: &str) -> String {
    if let Some(raw_plan) = plan_json {
        if let Ok(plan) = serde_json::from_str::<AutomationPlan>(raw_plan) {
            if !plan.runtime_prompt.trim().is_empty() {
                return plan.runtime_prompt.trim().to_string();
            }
        }
    }
    normalize_runtime_prompt(raw_prompt)
}

/// Build runtime input preferring the compiled plan runtime prompt.
pub fn build_runtime_run_input_from_plan(plan_json: Option<&str>, raw_prompt: &str) -> String {
    let task = runtime_task_from_plan_or_prompt(plan_json, raw_prompt);
    if task.is_empty() {
        return task;
    }

    format!(
        "AUTOMATION EXECUTION MODE\n\
This automation already exists. Do not create or modify any automation.\n\
Do not return setup confirmations.\n\
Execute the task now using available tools and return only this run output.\n\
If the task involves reading/checking emails, call `read_email_inbox` before declaring missing access.\n\n\
TASK:\n{task}"
    )
}

/// Build an effective prompt for execution from an AutomationRow.
/// For multi-step automations whose prompt is a generic placeholder,
/// compose a meaningful prompt from the workflow steps.
pub fn build_effective_prompt_from_row(automation: &crate::storage::AutomationRow) -> String {
    if let Some(ref steps_json) = automation.workflow_steps_json {
        #[derive(Deserialize)]
        struct Step {
            name: String,
            instruction: String,
        }
        if let Ok(steps) = serde_json::from_str::<Vec<Step>>(steps_json) {
            if !steps.is_empty() {
                let mut prompt = format!(
                    "Execute automation \"{}\" with these steps:\n",
                    automation.name
                );
                for (i, step) in steps.iter().enumerate() {
                    prompt.push_str(&format!("{}. {}: {}\n", i + 1, step.name, step.instruction));
                }
                return prompt;
            }
        }
    }
    automation.prompt.clone()
}

/// A declared dependency for an automation plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationDependency {
    /// Dependency category (`skill` | `mcp`).
    pub kind: String,
    /// Dependency identifier (skill name or MCP server name).
    pub name: String,
}

/// Serializable automation plan persisted with each automation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationPlan {
    pub version: i64,
    pub runtime_prompt: String,
    pub dependencies: Vec<AutomationDependency>,
}

/// Result of plan compilation + validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledAutomationPlan {
    pub plan: AutomationPlan,
    pub validation_errors: Vec<String>,
}

impl CompiledAutomationPlan {
    pub fn is_valid(&self) -> bool {
        self.validation_errors.is_empty()
    }

    pub fn plan_json(&self) -> String {
        serde_json::to_string(&self.plan).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn dependencies_json(&self) -> String {
        serde_json::to_string(&self.plan.dependencies).unwrap_or_else(|_| "[]".to_string())
    }

    pub fn validation_errors_json(&self) -> Option<String> {
        if self.validation_errors.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&self.validation_errors).unwrap_or_else(|_| "[]".to_string()),
            )
        }
    }
}

/// Compile and validate an automation plan from prompt + current config.
///
/// Keeps runtime pipeline shared; only produces metadata for preflight and impact checks.
pub fn compile_automation_plan(
    prompt: &str,
    config: &crate::config::Config,
) -> CompiledAutomationPlan {
    let runtime_prompt = normalize_runtime_prompt(prompt);
    let dependencies = extract_dependencies(prompt);
    let validation_errors = validate_dependencies(&dependencies, config);
    let plan = AutomationPlan {
        version: 1,
        runtime_prompt,
        dependencies,
    };
    CompiledAutomationPlan {
        plan,
        validation_errors,
    }
}

/// Parse validation errors JSON into a vector.
pub fn parse_validation_errors_json(raw: Option<&str>) -> Vec<String> {
    raw.and_then(|v| serde_json::from_str::<Vec<String>>(v).ok())
        .unwrap_or_default()
}

/// True when `dependencies_json` includes `kind:name`.
pub fn dependencies_include(dependencies_json: &str, kind: &str, name: &str) -> bool {
    let expected_kind = kind.trim().to_ascii_lowercase();
    let expected_name = name.trim().to_ascii_lowercase();
    serde_json::from_str::<Vec<AutomationDependency>>(dependencies_json)
        .unwrap_or_default()
        .into_iter()
        .any(|d| {
            d.kind.trim().eq_ignore_ascii_case(&expected_kind)
                && d.name.trim().eq_ignore_ascii_case(&expected_name)
        })
}

fn extract_dependencies(prompt: &str) -> Vec<AutomationDependency> {
    let mut out: BTreeSet<(String, String)> = BTreeSet::new();

    // Skills from explicit references: [$name](...) or $name
    if let Ok(re_md_skill) = Regex::new(r"\[\$([A-Za-z0-9_-]+)\]\(") {
        for cap in re_md_skill.captures_iter(prompt) {
            if let Some(name) = cap.get(1).map(|m| m.as_str().trim()) {
                if !name.is_empty() {
                    out.insert(("skill".to_string(), name.to_string()));
                }
            }
        }
    }
    if let Ok(re_skill) = Regex::new(r"\$([A-Za-z0-9_-]+)") {
        for cap in re_skill.captures_iter(prompt) {
            if let Some(name) = cap.get(1).map(|m| m.as_str().trim()) {
                if !name.is_empty() {
                    out.insert(("skill".to_string(), name.to_string()));
                }
            }
        }
    }

    // MCP servers from tool references like "gmail__search_messages"
    if let Ok(re_mcp_tool) = Regex::new(r"\b([A-Za-z0-9_-]+)__[A-Za-z0-9_-]+\b") {
        for cap in re_mcp_tool.captures_iter(prompt) {
            if let Some(server) = cap.get(1).map(|m| m.as_str().trim()) {
                if !server.is_empty() {
                    out.insert(("mcp".to_string(), server.to_string()));
                }
            }
        }
    }
    // Optional explicit "mcp:server" / "mcp server"
    if let Ok(re_mcp_name) = Regex::new(r"(?i)\bmcp[:\s]+([A-Za-z0-9_-]+)\b") {
        for cap in re_mcp_name.captures_iter(prompt) {
            if let Some(server) = cap.get(1).map(|m| m.as_str().trim()) {
                if !server.is_empty() {
                    out.insert(("mcp".to_string(), server.to_string()));
                }
            }
        }
    }

    out.into_iter()
        .map(|(kind, name)| AutomationDependency { kind, name })
        .collect()
}

fn validate_dependencies(
    dependencies: &[AutomationDependency],
    config: &crate::config::Config,
) -> Vec<String> {
    let installed_skills = load_installed_skill_names();
    let enabled_mcp: HashSet<String> = config
        .mcp
        .servers
        .iter()
        .filter(|(_, cfg)| cfg.enabled)
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect();

    let mut errors = Vec::new();
    for dep in dependencies {
        match dep.kind.as_str() {
            "skill" => {
                if !installed_skills.contains(&dep.name.to_ascii_lowercase()) {
                    errors.push(format!("Missing skill dependency: {}", dep.name));
                }
            }
            "mcp" => {
                if !enabled_mcp.contains(&dep.name.to_ascii_lowercase()) {
                    errors.push(format!("Missing or disabled MCP dependency: {}", dep.name));
                }
            }
            _ => {}
        }
    }
    errors
}

// ═══════════════════════════════════════════════════════════════
// VISUAL FLOW GRAPH
// ═══════════════════════════════════════════════════════════════

/// A node in the visual flow graph.
///
/// Supported kinds:
/// - `trigger` — schedule trigger (cron, interval)
/// - `tool`, `skill`, `mcp` — external capability
/// - `llm` — LLM inference step
/// - `condition` — conditional branch (diamond shape)
/// - `transform` — data transformation
/// - `deliver` — output delivery
/// - `parallel` — fork/join gateway (diamond shape, splits into parallel branches)
/// - `subprocess` — references another workflow (expandable)
/// - `loop` — repeating block
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowNode {
    pub id: String,
    pub kind: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub meta: String,
    /// For `subprocess` nodes: the workflow ID being referenced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_ref: Option<String>,
    /// For `loop` nodes: max iterations or condition description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_spec: Option<String>,
}

/// A directed edge between two flow nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowEdge {
    pub from: String,
    pub to: String,
    /// Optional label for condition branches (e.g. "yes"/"no", "match"/"else").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Complete visual flow representation for an automation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowGraph {
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
}

/// Helper: create a simple flow node.
fn flow_node(id: impl Into<String>, kind: impl Into<String>, label: impl Into<String>) -> FlowNode {
    FlowNode {
        id: id.into(),
        kind: kind.into(),
        label: label.into(),
        meta: String::new(),
        workflow_ref: None,
        loop_spec: None,
    }
}

/// Helper: create a flow node with meta text.
fn flow_node_meta(
    id: impl Into<String>,
    kind: impl Into<String>,
    label: impl Into<String>,
    meta: impl Into<String>,
) -> FlowNode {
    FlowNode {
        id: id.into(),
        kind: kind.into(),
        label: label.into(),
        meta: meta.into(),
        workflow_ref: None,
        loop_spec: None,
    }
}

/// Helper: create a plain edge.
fn edge(from: impl Into<String>, to: impl Into<String>) -> FlowEdge {
    FlowEdge {
        from: from.into(),
        to: to.into(),
        label: None,
    }
}

/// Helper: create a labeled edge (for condition branches).
fn edge_labeled(
    from: impl Into<String>,
    to: impl Into<String>,
    label: impl Into<String>,
) -> FlowEdge {
    FlowEdge {
        from: from.into(),
        to: to.into(),
        label: Some(label.into()),
    }
}

/// Derive a visual flow graph from an AutomationRow's existing fields.
///
/// Generates a DAG: trigger → [condition] → [parallel deps | steps | llm] → deliver.
/// Detects patterns:
/// - Multiple dependencies → parallel gateway fork/join
/// - Workflow steps with `approval_required` → condition gate before that step
/// - Trigger conditions → labeled branch edges (match / else)
pub fn derive_flow(row: &crate::storage::AutomationRow) -> FlowGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // 1. Trigger node
    nodes.push(flow_node(
        "trigger",
        "trigger",
        schedule_display_label(&row.schedule),
    ));
    let mut last_id = "trigger".to_string();

    // 2. Condition node (if not "always") — with labeled edges
    if row.trigger_kind != "always" {
        let cond_id = "cond".to_string();
        nodes.push(flow_node_meta(
            &cond_id,
            "condition",
            condition_label(&row.trigger_kind),
            row.trigger_value.clone().unwrap_or_default(),
        ));
        edges.push(edge(&last_id, &cond_id));
        last_id = cond_id;
    }

    // 3. Task nodes — from workflow steps or single LLM task
    if let Some(ref steps_json) = row.workflow_steps_json {
        if let Ok(steps) = serde_json::from_str::<Vec<WorkflowStepMini>>(steps_json) {
            for (i, step) in steps.iter().enumerate() {
                let step_id = format!("step_{i}");

                // If step requires approval, insert a condition gate before it
                if step.approval_required {
                    let gate_id = format!("gate_{i}");
                    nodes.push(flow_node(&gate_id, "condition", "Approval?"));
                    edges.push(edge(&last_id, &gate_id));
                    edges.push(edge_labeled(&gate_id, &step_id, "approved"));
                    last_id = gate_id.clone();
                }

                nodes.push(flow_node(&step_id, "llm", truncate_str(step.name.trim(), 28, "\u{2026}")));
                edges.push(edge(&last_id, &step_id));
                last_id = step_id;
            }
        }
    }

    // If no workflow steps were added, handle single-prompt automation
    let has_step_nodes = nodes.iter().any(|n| n.id.starts_with("step_"));
    if !has_step_nodes {
        let deps: Vec<AutomationDependency> =
            serde_json::from_str(&row.dependencies_json).unwrap_or_default();

        if deps.len() > 1 {
            // Multiple dependencies → parallel fork/join pattern
            let fork_id = "fork".to_string();
            nodes.push(flow_node(&fork_id, "parallel", "Fork"));
            edges.push(edge(&last_id, &fork_id));

            let join_id = "join".to_string();
            for (i, dep) in deps.iter().enumerate() {
                let dep_id = format!("dep_{i}");
                let kind = match dep.kind.as_str() {
                    "skill" => "skill",
                    "mcp" => "mcp",
                    _ => "tool",
                };
                nodes.push(flow_node(&dep_id, kind, &dep.name));
                edges.push(edge(&fork_id, &dep_id));
                edges.push(edge(&dep_id, &join_id));
            }

            nodes.push(flow_node(&join_id, "parallel", "Join"));
            last_id = join_id;
        } else {
            // 0 or 1 dependency → sequential
            for (i, dep) in deps.iter().enumerate() {
                let dep_id = format!("dep_{i}");
                let kind = match dep.kind.as_str() {
                    "skill" => "skill",
                    "mcp" => "mcp",
                    _ => "tool",
                };
                nodes.push(flow_node(&dep_id, kind, &dep.name));
                edges.push(edge(&last_id, &dep_id));
                last_id = dep_id;
            }
        }

        let task_id = "task".to_string();
        nodes.push(flow_node(&task_id, "llm", truncate_str(row.prompt.trim(), 28, "\u{2026}")));
        edges.push(edge(&last_id, &task_id));
        last_id = task_id;
    }

    // 4. Deliver node
    if let Some(ref deliver) = row.deliver_to {
        if !deliver.is_empty() {
            nodes.push(flow_node(
                "deliver",
                "deliver",
                deliver_channel_label(deliver),
            ));
            edges.push(edge(&last_id, "deliver"));
        }
    }

    FlowGraph { nodes, edges }
}

/// Minimal step struct just for flow derivation.
#[derive(Debug, Deserialize)]
struct WorkflowStepMini {
    name: String,
    #[serde(default)]
    approval_required: bool,
}

fn schedule_display_label(schedule: &str) -> String {
    if let Some(expr) = schedule.strip_prefix("cron:") {
        format!("Cron {}", expr.trim())
    } else if let Some(secs) = schedule.strip_prefix("every:") {
        let s: u64 = secs.parse().unwrap_or(0);
        if s >= 3600 {
            format!("Every {}h", s / 3600)
        } else if s >= 60 {
            format!("Every {}m", s / 60)
        } else {
            format!("Every {s}s")
        }
    } else {
        "Schedule".into()
    }
}

fn condition_label(trigger_kind: &str) -> String {
    match trigger_kind {
        "new_content" => "New Content?".into(),
        "value_changed" => "Changed?".into(),
        "threshold" => "Threshold".into(),
        other => other.to_string(),
    }
}

fn deliver_channel_label(channel: &str) -> String {
    match channel {
        "telegram" => "Telegram".into(),
        "discord" => "Discord".into(),
        "slack" => "Slack".into(),
        "whatsapp" => "WhatsApp".into(),
        "web" => "Web UI".into(),
        ch if ch.starts_with("email:") => "Email".into(),
        "email" => "Email".into(),
        other => other.to_string(),
    }
}


fn load_installed_skill_names() -> HashSet<String> {
    let skills_dir = Config::skills_dir();
    let Ok(entries) = std::fs::read_dir(skills_dir) else {
        return HashSet::new();
    };

    entries
        .flatten()
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|t| t.is_dir())
                .and_then(|_| entry.file_name().into_string().ok())
        })
        .map(|name| name.to_ascii_lowercase())
        .collect()
}

fn normalize_cron_expr(expr: &str) -> Result<String> {
    let expr = expr.trim();
    if expr.is_empty() {
        bail!("Cron expression cannot be empty");
    }

    let parts: Vec<&str> = expr.split_whitespace().collect();
    match parts.len() {
        5 => Ok(parts.join(" ")),
        6 if parts[2] == "*" => {
            // Common LLM mistake: append an extra trailing wildcard to a valid 5-field cron.
            // Example: "0 8 * * * *" -> "0 8 * * *"
            Ok(parts[..5].join(" "))
        }
        6 if parts[0] == "0" => {
            // Alternate format with leading seconds field.
            // Example: "0 0 8 * * *" -> "0 8 * * *"
            Ok(parts[1..].join(" "))
        }
        6 => bail!(
            "Unsupported 6-field cron '{expr}'. Use 5 fields (MIN HOUR DOM MON DOW), e.g. '0 8 * * *'."
        ),
        n => bail!(
            "Invalid cron expression: expected 5 fields (MIN HOUR DOM MON DOW), got {n} in '{expr}'."
        ),
    }
}

fn parse_last_run(value: Option<&str>) -> Option<DateTime<Utc>> {
    let raw = value?;
    if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return Some(naive.and_utc());
    }
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

// ═══════════════════════════════════════════════════════════════
// SHARED TRIGGER EVALUATION + RUN COMPLETION
// ═══════════════════════════════════════════════════════════════

/// Evaluate the automation trigger, complete the run, and update the automation status.
///
/// Shared by:
/// - Gateway post-processing (prompt-based automations)
/// - WorkflowEngine completion callback (workflow-based automations)
///
/// Returns `(should_notify, trigger_note)` — caller decides whether to send outbound.
pub async fn evaluate_and_complete_automation_run(
    db: &crate::storage::Database,
    automation_id: &str,
    run_id: &str,
    run_output: &str,
    is_error: bool,
) -> Result<(bool, Option<String>)> {
    let run_result = if is_error {
        format!("Run failed: {run_output}")
    } else {
        run_output.to_string()
    };
    let run_status = if is_error { "error" } else { "success" };
    let automation_status = if is_error { "error" } else { "active" };

    // Evaluate trigger condition (only for successful runs)
    let mut should_notify = true;
    let mut trigger_note: Option<String> = None;

    if !is_error {
        if let Ok(Some(automation)) = db.load_automation(automation_id).await {
            let previous_result = db
                .load_last_successful_automation_result(automation_id, Some(run_id))
                .await
                .ok()
                .flatten();
            let (notify, note) = evaluate_automation_trigger(
                &automation.trigger_kind,
                automation.trigger_value.as_deref(),
                previous_result.as_deref(),
                run_output,
            );
            should_notify = notify;
            trigger_note = note;
        }
    }

    // Complete the run
    if let Err(e) = db
        .complete_automation_run(run_id, run_status, Some(&run_result))
        .await
    {
        tracing::error!(error = %e, run_id = %run_id, "Failed to complete automation run");
    }

    // Update automation status
    let latest_result = if is_error {
        truncate_str(&run_result, 500, "...")
    } else if let Some(ref note) = trigger_note {
        format!("{note} | output: {}", truncate_str(&run_result, 300, "..."))
    } else {
        truncate_str(&run_result, 500, "...")
    };

    if let Err(e) = db
        .update_automation(
            automation_id,
            crate::storage::AutomationUpdate {
                status: Some(automation_status.to_string()),
                last_result: Some(Some(latest_result)),
                touch_last_run: true,
                ..Default::default()
            },
        )
        .await
    {
        tracing::error!(
            error = %e,
            automation_id = %automation_id,
            "Failed to update automation status after run"
        );
    }

    Ok((should_notify, trigger_note))
}

/// Evaluate whether an automation trigger condition is satisfied.
///
/// Returns `(should_notify, optional_note)`.
pub fn evaluate_automation_trigger(
    trigger_kind: &str,
    trigger_value: Option<&str>,
    previous_result: Option<&str>,
    current_result: &str,
) -> (bool, Option<String>) {
    match trigger_kind.trim().to_ascii_lowercase().as_str() {
        "always" => (true, None),
        "on_change" | "changed" => {
            let Some(previous) = previous_result else {
                return (
                    true,
                    Some("No previous successful run; notifying".to_string()),
                );
            };
            let previous_norm = normalize_for_trigger_compare(previous);
            let current_norm = normalize_for_trigger_compare(current_result);
            if previous_norm == current_norm {
                (
                    false,
                    Some("Trigger on_change not matched (result unchanged)".to_string()),
                )
            } else {
                (true, None)
            }
        }
        "contains" => {
            let needle = trigger_value.unwrap_or("").trim();
            if needle.is_empty() {
                return (
                    true,
                    Some("Trigger contains misconfigured; defaulting to notify".to_string()),
                );
            }
            let haystack = current_result.to_ascii_lowercase();
            if haystack.contains(&needle.to_ascii_lowercase()) {
                (true, None)
            } else {
                (
                    false,
                    Some(format!("Trigger contains not matched ('{needle}')")),
                )
            }
        }
        other => (
            true,
            Some(format!("Unknown trigger '{other}'; defaulting to notify")),
        ),
    }
}

fn normalize_for_trigger_compare(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_cron_accepts_five_fields() {
        let schedule = AutomationSchedule::from_cron("0 8 * * *").unwrap();
        assert_eq!(schedule.as_stored(), "cron:0 8 * * *");
    }

    #[test]
    fn test_from_cron_normalizes_trailing_wildcard() {
        let schedule = AutomationSchedule::from_cron("0 8 * * * *").unwrap();
        assert_eq!(schedule.as_stored(), "cron:0 8 * * *");
    }

    #[test]
    fn test_from_cron_normalizes_leading_seconds_format() {
        let schedule = AutomationSchedule::from_cron("0 0 8 * * *").unwrap();
        assert_eq!(schedule.as_stored(), "cron:0 8 * * *");
    }

    #[test]
    fn test_from_cron_rejects_unsupported_six_field() {
        let err = AutomationSchedule::from_cron("30 8 10 * * *").unwrap_err();
        assert!(err.to_string().contains("Unsupported 6-field"));
    }

    #[test]
    fn test_next_run_from_stored_cron_five_fields() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 3, 4)
            .unwrap()
            .and_hms_opt(7, 30, 0)
            .unwrap()
            .and_utc();
        let next = AutomationSchedule::next_run_from_stored("cron:0 8 * * *", None, now);
        assert!(next.is_some());
    }

    #[test]
    fn test_normalize_runtime_prompt_from_creation_phrase() {
        let prompt = "Crea una automation chiamata Riepilogo: controlla le email non lette e inviami un riassunto";
        let normalized = normalize_runtime_prompt(prompt);
        assert_eq!(
            normalized,
            "controlla le email non lette e inviami un riassunto"
        );
    }

    #[test]
    fn test_build_runtime_run_input_contains_execution_guard() {
        let prompt = "Crea una automation: controlla le email non lette";
        let wrapped = build_runtime_run_input(prompt);
        assert!(wrapped.contains("AUTOMATION EXECUTION MODE"));
        assert!(wrapped.contains("Do not create or modify any automation"));
        assert!(wrapped.contains("controlla le email non lette"));
    }

    #[test]
    fn test_normalize_runtime_prompt_from_structured_confirmation() {
        let prompt = r#"Fatto! ✅
Creata automazione "Riepilogo email giornaliero":
- Orario: 8:00 ogni giorno
- Canale: Telegram
- Azione: Controlla le email non lette, classificale e invia un riassunto"#;
        let normalized = normalize_runtime_prompt(prompt);
        assert_eq!(
            normalized,
            "Controlla le email non lette, classificale e invia un riassunto"
        );
    }

    #[test]
    fn test_build_runtime_from_plan_prefers_runtime_prompt() {
        let plan = AutomationPlan {
            version: 1,
            runtime_prompt: "check inbox and summarize".to_string(),
            dependencies: vec![],
        };
        let plan_json = serde_json::to_string(&plan).unwrap();
        let wrapped = build_runtime_run_input_from_plan(
            Some(&plan_json),
            "Crea una automation: this should be ignored",
        );
        assert!(wrapped.contains("check inbox and summarize"));
        assert!(!wrapped.contains("this should be ignored"));
    }

    #[test]
    fn test_dependencies_include_matches_kind_and_name() {
        let raw = r#"[{"kind":"skill","name":"checks"},{"kind":"mcp","name":"gmail"}]"#;
        assert!(dependencies_include(raw, "skill", "checks"));
        assert!(dependencies_include(raw, "mcp", "gmail"));
        assert!(!dependencies_include(raw, "skill", "missing"));
    }

    #[test]
    fn test_parse_validation_errors_json() {
        let errs = parse_validation_errors_json(Some(r#"["a","b"]"#));
        assert_eq!(errs, vec!["a".to_string(), "b".to_string()]);
        let empty = parse_validation_errors_json(Some("not-json"));
        assert!(empty.is_empty());
    }
}
