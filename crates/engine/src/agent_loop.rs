//! The single guarded ReAct loop — motore #1 (ADR 0021), extracted into the engine (ADR 0024,
//! increment 5, Point 5 / 5.D1c.10).
//!
//! This is the canonical agent turn: a bounded round loop that calls the model, dispatches its tool
//! calls through the seams, tracks plan progress, and synthesizes a final answer. It is GENERIC over
//! its collaborators (the `contract` seams) so the engine stays decoupled from the gateway's concrete
//! `AppState`/transport — the gateway builds the impls and injects them.
//!
//! Convergence (ADR 0024, 5.D2, `50ed7ec6`): this IS the one loop. The gateway's inline copy and the
//! `HOMUN_ENGINE_CRATE` flag are deleted — `run_agent_rounds` builds the seams and calls `run_turn`
//! unconditionally (per "converge, don't duplicate"). ADR 0025 (browse-as-recursion) invokes this same
//! loop recursively for the browser sub-agent.

use crate::browser::{
    is_browser_granular_tool, is_stale_ref_recovery_result, prune_browser_history,
    resolve_browser_chat_tool_name,
};
use crate::contract::{
    BrowserExecutor, CapabilityExecutor, ContextCompactor, EventSink, ExecutionJournal,
    ModelClient, PlanProgress, TurnCompletionJudge, TurnControlDecision, TurnControlDisposition,
    TurnPolicy,
};
use crate::events::{GenerateStreamEvent, TokenMetrics};
use crate::execution_journal::{
    AgentExecutionEvent, classify_tool_result, external_action_evidence_marker, tool_family,
    tool_result_fingerprint,
};
use crate::hitl::{
    HitlEnvelope, HitlKind, NoToolsClassification, classify_no_tools_stop,
    ensure_free_hitl_marker_in_text, finalize_terminal_text_for_hitl,
};
use crate::markers::{
    append_vault_reveal_marker_if_missing, extract_vault_reveal_marker,
    preserved_display_marker_blocks, strip_display_markers, text_awaits_user, visible_answer,
};
use crate::model_normalize;
use crate::plan::{
    advance_plan_frontier, build_plan_markdown, collapse_plan_markers, plan_next_open,
    plan_step_id, plan_step_status, plan_step_title, plan_value_goal, plan_value_steps,
    replace_latest_plan_marker, should_nudge_for_open_plan,
};
use crate::text::{extract_source_urls, fonti_section, is_low_value_source_url};
use crate::tools::{connected_capability_execution_trace_line, summarize_tool_action};
use crate::{LoopState, TurnConfig};
use std::time::Instant;

/// Max harness "you're not done — plan the rest" nudges per turn before giving up (F1 anti-loop).
const MAX_PLAN_NUDGES: u32 = 8;
/// One repair nudge when the model asks a closed choice in prose without a CHOICES card.
const MAX_CHOICES_CARD_NUDGES: u32 = 1;
/// One repair nudge when the model asks for free-text fields without a CLARIFY card.
const MAX_CLARIFY_CARD_NUDGES: u32 = 1;
/// One repair nudge when the model mentions Payment Approval Card without the hold card.
const MAX_PAYMENT_CARD_NUDGES: u32 = 1;
/// One repair round when a resumed turn tries to reopen the wait that its wake just resolved.
const MAX_RESOLVED_HITL_NUDGES: u32 = 1;

/// Repeats of an identical round before the loop TELLS the model to change approach. Repetition means
/// the model is stuck on one step, not that the task is impossible: the useful response is a specific
/// hint ("you have called exactly this N times; target something else"), not ending the turn. An
/// autonomous run is allowed to take a long time — what must not happen is spending it on the same
/// failing call.
const REPEAT_NUDGE_AT: u32 = 2;
/// Repeats before giving up. Only reached AFTER the change-approach hint above, i.e. the model was
/// told exactly what was looping and did it again anyway.
const REPEAT_STOP_AT: u32 = 4;

/// Bounded wait, in 50ms wall-clock ticks, for an uninterpreted steering row at the finalization
/// fence before parking (≈ 40 × 50ms ≈ 2s). Deliberately wall-clock, not event-driven: the row may
/// never become interpretable (the semantic model can be down indefinitely), so the budget must
/// elapse on its own — replaces the old infinite spin, which parks instead of hanging.
const PARK_WAIT_CYCLES: u32 = 40;

async fn wait_for_interrupting_control<M: ModelClient>(model_client: &M) -> TurnControlDecision {
    loop {
        if let Some(control) = model_client.current_turn_control()
            && control.disposition != TurnControlDisposition::ContinueCurrentWork
        {
            return control;
        }
        let control = model_client.wait_for_turn_control().await;
        if control.disposition != TurnControlDisposition::ContinueCurrentWork {
            return control;
        }
    }
}

fn apply_turn_control<M: ModelClient>(
    model_client: &M,
    messages: &mut Vec<serde_json::Value>,
    control: &TurnControlDecision,
) {
    messages.push(serde_json::json!({
        "role": "user",
        "content": format!(
            "[VALIDATED USER STEERING]\n{}\n\nApply the structured runtime disposition {:?} now.",
            control.instruction,
            control.disposition,
        ),
    }));
    model_client.acknowledge_turn_control_applied(control.steering_id);
}

/// Acknowledge every drained (applied) steering id back to the coordinator, deduped. Shared
/// between the normal turn-end completion and the park early-return: a control applied EARLIER
/// in the turn (round loop, model-call race, or an earlier fence-drain iteration) must still be
/// completed even when the turn parks on a DIFFERENT, later, uninterpreted row — otherwise that
/// earlier id's store row is stuck at `applied` forever (a permanent "Applying…" spinner in the
/// UI). Drains the vec so callers can't double-flush the same ids.
fn complete_drained_steering<M: ModelClient>(
    model_client: &M,
    steering_to_complete: &mut Vec<i64>,
) {
    steering_to_complete.sort_unstable();
    steering_to_complete.dedup();
    for steering_id in steering_to_complete.drain(..) {
        model_client.acknowledge_turn_control_completed(steering_id);
    }
}

/// Harness-driven plan progress from VERIFIED evidence. Called after each work tool result: if the
/// plan has a frontier `doing` step and enough new evidence has accrued, ask the F2 judge whether that
/// step is genuinely complete; if so, mark it done, advance the frontier, and emit the ‹‹PLAN›› card +
/// persist — the same canonical live+durable update the model's own `step_advance` produces.
///
/// KEPT past ADR 0025 (which retired the browser model-switch) because it is NOT a browser band-aid: it
/// is the general weak-model safety net. A capable manager calls `step_advance` itself, but Homun also
/// runs on weak LOCAL models as the driver (ADR 0016/0018), which don't reliably self-advance — this is
/// how their multi-step plans progress. Verified-only, so a stuck step never fakes progress; stride-gated
/// so the (cheap `memory`-role) judge runs at most once per few tool results. Gated off entirely for the
/// browse sub-loop (its `TurnConfig` disables autoadvance) — the manager owns plan control-flow now.
///
/// Expressed over the seams (`PlanProgress` for verify/record/persist + the steps→Value bridge,
/// `EventSink` for the card, `TurnConfig` for the flags) — no `AppState`/transport/`ExecutionPlan`.
#[derive(Clone, Copy)]
struct EvidenceVerification {
    round: usize,
    force: bool,
}

async fn try_advance_frontier_from_evidence(
    ls: &mut LoopState,
    plan_progress: &impl PlanProgress,
    execution_journal: &impl ExecutionJournal,
    event_sink: &impl EventSink,
    cfg: &TurnConfig,
    thread_id: Option<&str>,
    verification: EvidenceVerification,
) {
    if !cfg.autoadvance_from_evidence || !cfg.step_verification {
        return;
    }
    // Evidence stride: only re-check after a few new tool outcomes since the last attempt.
    const EVIDENCE_STRIDE: usize = 3;
    if !verification.force
        && ls.step_evidence.len() < ls.progress_verify_anchor.saturating_add(EVIDENCE_STRIDE)
    {
        return;
    }
    ls.progress_verify_anchor = ls.step_evidence.len();
    let batch_evidence = if verification.force {
        let mut prioritized = Vec::new();
        if let Some(candidate) = ls.step_evidence.last() {
            prioritized.push(candidate.clone());
        }
        prioritized.extend(ls.step_evidence.iter().rev().skip(1).take(8).cloned());
        prioritized.join("\n")
    } else {
        ls.step_evidence.join("\n")
    };
    if batch_evidence.is_empty() {
        return;
    }
    // Normal tool-result checks remain conservative. A candidate final answer can contain several
    // analytical deliverables, so verify up to four steps individually before deciding to nudge.
    // This is not a delivery sweep: every step still passes the fail-closed F2 judge.
    let max_advances = if verification.force { 4 } else { 2 };
    for _ in 0..max_advances {
        let plan_steps = plan_value_steps(&ls.plan);
        let Some(idx) = plan_steps
            .iter()
            .position(|s| plan_step_status(s) == "doing")
        else {
            return; // nothing in progress (plan complete or not started)
        };
        let title = plan_step_title(&plan_steps[idx]).to_string();
        let criterion = plan_steps[idx]
            .get("done_criterion")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let (ok, reason) = plan_progress
            .verify_step_complete(&title, &criterion, &batch_evidence)
            .await;
        if !ok {
            if cfg.verbose {
                eprintln!("[plan] F2 kept step open: «{title}» — {reason}");
            }
            return; // frontier step not proven done yet → leave it doing
        }
        let mut plan_steps = plan_steps;
        advance_plan_frontier(&mut plan_steps);
        let verified_step = plan_steps[idx].clone();
        // record_step_outcome clones the evidence internally (its impl offloads to spawn_blocking),
        // so pass the current window by ref — the same evidence the old inline clone captured pre-clear.
        plan_progress
            .record_step_outcome(thread_id, &verified_step, &ls.step_evidence)
            .await;
        // The plan's goal rides the canonical Value; carry it across the step-only rebuild so the
        // frontier advance never drops it.
        let plan_goal = plan_value_goal(&ls.plan);
        ls.plan = plan_progress.plan_value_from_steps(plan_goal.as_deref(), &plan_steps);
        ls.progress_anchor_round = verification.round; // F1: real progress → reset the stall guard
        event_sink
            .emit(GenerateStreamEvent::Delta {
                text: format!(
                    "‹‹ACT››✓ Step verified: {}‹‹/ACT››",
                    title.chars().take(60).collect::<String>()
                ),
            })
            .await;
        // Per-step visible event (frontend contract): the harness just closed this step, F2-verified.
        // Rides the same stream channel as the ‹‹ACT››✓ delta above — the gateway's
        // `turn_event_from_stream_value` maps it to the durable `step_advance` turn event.
        event_sink
            .emit(GenerateStreamEvent::StepAdvance {
                step_id: plan_step_id(&plan_steps[idx])
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("s{}", idx + 1)),
                title: title.clone(),
                from: Some("doing".to_string()),
                to: "done".to_string(),
                verified: Some(true),
                note: None,
            })
            .await;
        // The canonical live plan card (→ plan_update event) + durable runtime plan, exactly like
        // the model's own step_advance path.
        let plan_mark = format!(
            "‹‹PLAN››{}‹‹/PLAN››",
            build_plan_markdown(plan_goal.as_deref(), &plan_steps)
        );
        event_sink
            .emit(GenerateStreamEvent::Delta { text: plan_mark })
            .await;
        plan_progress
            .persist_plan(thread_id, plan_goal.as_deref(), &plan_steps)
            .await;
        execution_journal.record(AgentExecutionEvent::PlanUpdated {
            round: verification.round,
            source: "verified_evidence".to_string(),
        });
        // This evidence window was consumed by the advance — reset it + the stride anchor.
        ls.step_evidence.clear();
        ls.progress_verify_anchor = 0;
    }
}

fn candidate_answer_evidence(content: &str) -> Option<String> {
    let visible = strip_display_markers(content).trim().to_string();
    if visible.chars().count() < 40 {
        return None;
    }
    Some(format!(
        "assistant_candidate_output (direct evidence only for an analytical deliverable; never proof that an external action or command ran) → {}",
        visible.chars().take(3200).collect::<String>()
    ))
}

fn evidence_argument_provenance(name: &str, args_raw: &str) -> Option<String> {
    if name == "run_in_sandbox" {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(args_raw).ok()?;
    const SAFE_KEYS: [&str; 5] = ["path", "url", "goal", "query", "intent"];
    let mut fields = Vec::new();
    for key in SAFE_KEYS {
        let Some(raw) = value.get(key).and_then(serde_json::Value::as_str) else {
            continue;
        };
        let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty() {
            continue;
        }
        fields.push(format!(
            "{key}={}",
            normalized.chars().take(240).collect::<String>()
        ));
    }
    (!fields.is_empty()).then(|| fields.join(", "))
}

fn is_plan_bookkeeping_tool(name: &str) -> bool {
    matches!(name, "update_plan" | "step_advance")
}

fn normalize_tool_call_ids_for_round(round: usize, calls: &mut [serde_json::Value]) {
    let mut seen = std::collections::BTreeSet::new();
    for (idx, call) in calls.iter_mut().enumerate() {
        let raw_id = call
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let provider_synthesized = raw_id.is_empty() || raw_id.starts_with("ollama_call_");
        let duplicate = !provider_synthesized && !seen.insert(raw_id.to_string());
        if !(provider_synthesized || duplicate) {
            continue;
        }

        let prefix = if raw_id.is_empty() {
            "tool_call".to_string()
        } else {
            raw_id
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                        ch
                    } else {
                        '_'
                    }
                })
                .take(64)
                .collect::<String>()
        };
        let mut next_id = format!("{prefix}_round_{round}_{idx}");
        let mut salt = 0usize;
        while !seen.insert(next_id.clone()) {
            salt += 1;
            next_id = format!("{prefix}_round_{round}_{idx}_{salt}");
        }
        if let Some(obj) = call.as_object_mut() {
            obj.insert("id".to_string(), serde_json::Value::String(next_id));
        }
    }
}

/// Tools that are introspective or planning-related — the plan-before-act gate must NOT fire for
/// these because they are part of the planning/memory/discovery cycle, not "work" that acts on the
/// world. Distinct from [`is_plan_bookkeeping_tool`] which is narrower (only `update_plan` /
/// `step_advance`) and is used for evidence-tracking exclusion.
fn is_introspective_tool(name: &str) -> bool {
    matches!(
        name,
        "update_plan"
            | "step_advance"
            | "recall_memory"
            | "find_capability"
            | "suggest_capabilities"
            | "use_skill"
    )
}

fn capability_runtime_tool_result_payload(
    name: &str,
    effects: &crate::ToolEffects,
) -> Option<serde_json::Value> {
    if effects.load_tools.is_empty()
        && effects.arm_sensitive.is_empty()
        && effects.pending_capability.is_none()
        && effects.blocked_capabilities.is_empty()
    {
        return None;
    }
    let loaded_tools = effects
        .load_tools
        .iter()
        .map(|tool| tool.key.as_str())
        .collect::<Vec<_>>();
    Some(serde_json::json!({
        "name": name,
        "capability_runtime": {
            "loaded_tools": loaded_tools,
            "armed_sensitive_domains": effects.arm_sensitive,
            "pending_capability": effects.pending_capability,
            "blocked_capabilities": effects.blocked_capabilities,
        }
    }))
}

/// Rough count of imperative signals in a user message: exclamation marks plus distinct
/// action-verb matches. Used by [`request_is_complex`] as a proxy for "2+ imperative sentences".
/// Pure heuristic — never the sole gate (the length and multi-tool conditions also apply).
fn count_imperative_signals(text: &str) -> usize {
    let exclamatory = text.matches('!').count();
    const ACTION_VERBS: &[&str] = &[
        "create",
        "write",
        "build",
        "generate",
        "run",
        "execute",
        "update",
        "delete",
        "send",
        "make",
        "deploy",
        "configure",
        "install",
        "download",
        "analyze",
        "implement",
        "fix",
        "refactor",
        "test",
        "compile",
        "search",
        "find",
        "read",
        "edit",
        "move",
        "copy",
        "convert",
        "transform",
        "add",
        "remove",
        "check",
        "verify",
        "scan",
        "extract",
        "set up",
    ];
    let text_lower = text.to_lowercase();
    let verb_count = ACTION_VERBS
        .iter()
        .filter(|verb| text_lower.contains(*verb))
        .count();
    exclamatory + verb_count
}

/// The plan-before-act gate's complexity heuristic. Only activate the gate when the request
/// seems non-trivial, so simple one-shot interactions pass straight through. Three independent
/// conditions (any one suffices):
/// 1. User message longer than 80 characters.
/// 2. 2+ imperative signals (exclamation marks or action-verb matches — see [`count_imperative_signals`]).
/// 3. The model already called 2+ different tools in this round (a multi-step signal).
///
/// Pure (no IO); tested independently.
fn request_is_complex(user_message: &str, calls: &[serde_json::Value]) -> bool {
    if user_message.trim().chars().count() > 80 {
        return true;
    }
    if count_imperative_signals(user_message) >= 2 {
        return true;
    }
    let distinct_tools: std::collections::BTreeSet<&str> = calls
        .iter()
        .filter_map(|c| {
            c.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
        })
        .collect();
    distinct_tools.len() >= 2
}

fn browser_exhausted_fallback_answer(
    loop_exit: Option<&str>,
    grounded_browse: Option<&crate::BrowseResult>,
    sources: &[String],
) -> String {
    let reason = match loop_exit {
        Some("browser_budget_exceeded") => "il browser ha esaurito il budget di navigazione",
        Some("structured_no_progress") => {
            "il browser non ha fatto progressi utili dopo piu tentativi"
        }
        Some("round_ceiling_reached") => "il turno ha raggiunto il limite di passaggi disponibili",
        _ => "la ricerca browser non ha prodotto risultati verificabili",
    };
    if let Some(result) = grounded_browse
        && result.found
    {
        let mut answer = format!(
            "La ricerca browser ha raccolto risultati parziali, ma la sintesi finale non e stata completata perche {reason}. Non ho effettuato prenotazioni o acquisti."
        );
        if !result.answer.trim().is_empty() {
            answer.push_str("\n\nRisultati osservati:\n");
            answer.push_str(result.answer.trim());
        } else if !result.items.is_empty() {
            answer.push_str("\n\nRisultati osservati:\n");
            answer.push_str(
                &serde_json::to_string_pretty(&result.items)
                    .unwrap_or_else(|_| format!("{:?}", result.items)),
            );
        }
        if !result.fields_missing.is_empty() {
            answer.push_str("\n\nCampi ancora mancanti:");
            for field in &result.fields_missing {
                answer.push_str("\n- ");
                answer.push_str(field);
            }
        }
        let mut all_sources = sources.to_vec();
        for source in &result.sources {
            if !all_sources.contains(source) {
                all_sources.push(source.clone());
            }
        }
        if !all_sources.is_empty() {
            answer.push_str("\n\nFonti:");
            for source in all_sources {
                answer.push_str("\n- ");
                answer.push_str(&source);
            }
        }
        return answer;
    }
    let mut answer = format!(
        "Non sono riuscito a leggere risultati verificabili: {reason}. Non ho effettuato prenotazioni o acquisti."
    );
    if !sources.is_empty() {
        answer.push_str("\n\nFonti tentate:");
        for source in sources {
            answer.push_str("\n- ");
            answer.push_str(source);
        }
    }
    answer
}

fn browser_incomplete_loop_exit(loop_exit: Option<&str>) -> bool {
    matches!(
        loop_exit,
        Some(
            "browser_budget_exceeded"
                | "structured_no_progress"
                | "round_budget_since_last_progress"
                | "browser_nav_cap_reached"
        )
    )
}

fn browser_incomplete_failure_code(loop_exit: Option<&str>) -> &'static str {
    match loop_exit {
        Some("browser_budget_exceeded") => "browser_budget_exceeded",
        Some("structured_no_progress") => "browser_structured_no_progress",
        Some("round_budget_since_last_progress") => "browser_round_budget_exceeded",
        Some("browser_nav_cap_reached") => "browser_navigation_cap_reached",
        _ => "browser_incomplete",
    }
}

fn tool_schema_name(schema: &serde_json::Value) -> Option<&str> {
    schema
        .get("function")
        .and_then(|function| function.get("name"))
        .and_then(serde_json::Value::as_str)
}

fn has_tool_schema(tools: &[serde_json::Value], name: &str) -> bool {
    tools
        .iter()
        .any(|schema| tool_schema_name(schema) == Some(name))
}

fn only_tool_schema(tools: &[serde_json::Value], name: &str) -> Vec<serde_json::Value> {
    tools
        .iter()
        .filter(|schema| tool_schema_name(schema) == Some(name))
        .cloned()
        .collect()
}

fn browser_incomplete_failure(
    loop_exit: Option<&str>,
) -> local_first_execution_protocol::ExecutionFailure {
    let code = browser_incomplete_failure_code(loop_exit);
    let detail = "The browser stopped before completing the requested work";
    match loop_exit {
        Some("structured_no_progress") => {
            local_first_execution_protocol::ExecutionFailure::permanent(code, detail)
        }
        _ => local_first_execution_protocol::ExecutionFailure::transient(code, detail),
    }
}

/// Run ONE agent turn: the bounded guarded ReAct loop + forced synthesis. GENERIC over the seams so
/// the engine stays decoupled from the gateway's `AppState`/transport — the gateway builds the impls
/// (constructed per turn) and injects them. Returns the [`crate::TurnOutcome`] the gateway's post-turn
/// tail (memory learn + code-graph refresh) consumes. Called unconditionally by `run_agent_rounds`
/// (ADR 0024 5.D2) — no flag, no inline copy.
#[allow(clippy::too_many_arguments)] // the turn's seams + engine-safe data; grouped further post-5.D2.
pub async fn run_turn<M, C, B, P, J, K, Pol, X, E>(
    mut ls: LoopState,
    cfg: TurnConfig,
    usage_context: &local_first_inference_usage::UsageContext,
    model_client: &M,
    capability_executor: &C,
    browser_executor: &mut B,
    plan_progress: &P,
    completion_judge: &J,
    compactor: &K,
    turn_policy: &Pol,
    execution_journal: &X,
    event_sink: &E,
    temperature: f64,
    // by-ref: these are also borrowed by the gateway-constructed executors (dispatch shares one
    // construction), so the loop borrows them too rather than moving them out from under the executors.
    thread_id: Option<&str>,
    composio_writes: &std::collections::BTreeSet<String>,
    catalog_index: &[(String, String, serde_json::Value)],
    memory_user_message: String,
    mut memory_answer: String,
    _last_model_error: Option<String>,
    mut final_done: bool,
    mut plan_nudges: u32,
    mut turn_used_tools: bool,
    mut browse_sources: Vec<String>,
    trace_dir: Option<std::path::PathBuf>,
    // Readable per-turn observability sink (ported). A pure recorder: every `record` only appends a
    // JSON line (or no-ops when disabled), NEVER gates a decision — behavior-preserving by construction.
    // Sub-turns (the `browse` recursion) pass `TurnTrace::disabled()` so they don't spam the trace.
    turn_trace: &crate::turn_trace::TurnTrace,
) -> crate::TurnOutcome
where
    M: ModelClient,
    C: CapabilityExecutor,
    B: BrowserExecutor,
    P: PlanProgress,
    J: TurnCompletionJudge,
    K: ContextCompactor,
    Pol: TurnPolicy,
    X: ExecutionJournal,
    E: EventSink,
{
    let mut visible_answer_delivered = false;
    let mut awaiting_envelope: Option<HitlEnvelope> = None;
    let mut effect_resolution_receipt = None;
    let mut choices_card_nudges: u32 = 0;
    let mut clarify_card_nudges: u32 = 0;
    let mut payment_card_nudges: u32 = 0;
    let mut resolved_hitl_nudges: u32 = 0;
    let mut grounded_browse_result: Option<crate::BrowseResult> = None;
    // Plan-approval gate: when a complex turn starts with a work tool and no
    // canonical plan, the loop asks the model to propose a plan for the user to
    // approve (PLAN_PROPOSE) instead of dispatching the tool unilaterally. Fires
    // at most once per turn; the model then proposes and stops for approval.
    let mut plan_approval_requested = false;
    // Task #8/#9: replan directive injection — fires at most once per turn. When consecutive
    // tool failures (same family or cross-family) exceed the threshold, inject a "revise the plan"
    // directive and continue the loop instead of stopping. Once fired, stays true so the model
    // gets exactly one replan chance per turn — subsequent failures fall through to the existing
    // stop-for-no-progress forced synthesis.
    let mut replan_injected_this_turn = false;
    let mut protocol_failure = None;
    let turn_started_at = Instant::now();
    // The per-progress stall clock: reset to `now` on every real browser progress (see the
    // `browser_no_progress = 0` point below). The browser wall-clock budget is measured from THIS,
    // not from `turn_started_at`, so a browse that keeps advancing is never choked by a cumulative
    // timer — only the absolute cap (from `turn_started_at`) and a genuine stall stop it.
    let mut last_browser_progress_at = Instant::now();
    let mut steering_to_complete = Vec::new();
    // Tracks the last round the loop actually ran, captured in outer scope because the `for`
    // loop's `round` binding does not survive past the loop (exhaustion or `break`) — the
    // post-loop finalization fence below needs it to checkpoint at the right round on park.
    // The `0` default is never observed as a stale/wrong round in practice: `cfg.hard_round_ceiling`
    // is always configured >= 1, so the loop runs at least round 0 (setting this) before the
    // fence can ever be reached.
    let mut last_round: usize = 0;
    // WHY the round loop stopped. The loop has 22 exits and the trace used to record only
    // `post_loop_exhausted` — "the loop ended" — which is why diagnosing a turn that stopped too
    // early meant guessing between them, and then tuning whichever limit seemed likeliest. Every
    // `break` out of `'rounds` sets this; `None` after the loop means the round range ran out (or,
    // if it did NOT, that someone added a `break` without a reason — reported as
    // `uninstrumented_exit` rather than silently mislabelled as exhaustion).
    let mut loop_exit: Option<&'static str> = None;
    'rounds: for round in 0..cfg.hard_round_ceiling {
        last_round = round;
        if let Some(control) = model_client.current_turn_control() {
            apply_turn_control(model_client, &mut ls.messages, &control);
            steering_to_complete.push(control.steering_id);
            match control.disposition {
                TurnControlDisposition::FinalizeWithCurrentEvidence => {
                    loop_exit = Some("steering_finalize_pre_model");
                    break 'rounds;
                }
                TurnControlDisposition::NeedsClarification => {
                    awaiting_envelope = Some(HitlEnvelope {
                        kind: HitlKind::Clarify,
                        hold_policy: crate::hitl::HoldPolicy::Free,
                        payload: serde_json::json!({}),
                        source_marker: "steering_clarify".into(),
                    });
                    loop_exit = Some("steering_needs_clarification_pre_model");
                    break 'rounds;
                }
                TurnControlDisposition::CancelCurrentWork => {
                    final_done = true;
                    loop_exit = Some("steering_cancel_pre_model");
                    break 'rounds;
                }
                TurnControlDisposition::ContinueCurrentWork
                | TurnControlDisposition::ReplanCurrentWork => {}
            }
        }
        if ls.browser_used {
            let elapsed_ms =
                u64::try_from(turn_started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
            let stall_ms =
                u64::try_from(last_browser_progress_at.elapsed().as_millis()).unwrap_or(u64::MAX);
            if let Some(reason) = cfg.browser_budget.stop_reason(
                elapsed_ms,
                stall_ms,
                ls.browser_failed_navigations,
                ls.browser_no_progress,
            ) {
                execution_journal.record(AgentExecutionEvent::BrowserBudgetExceeded {
                    round,
                    reason: reason.as_str().to_string(),
                    elapsed_ms,
                    failed_navigations: ls.browser_failed_navigations,
                    no_progress: ls.browser_no_progress,
                });
                let _ = event_sink
                    .emit(GenerateStreamEvent::Activity {
                        text: format!("browser_budget_exceeded:{}", reason.as_str()),
                    })
                    .await;
                loop_exit = Some("browser_budget_exceeded");
                break;
            }
        }
        let max_rounds = if ls.browser_used {
            cfg.browser_max_rounds
        } else {
            cfg.max_rounds
        };
        // Hard stop once the effective budget is reached (the forced-synthesis
        // fallback below still runs because `final_done` is false). The budget is
        // measured from the last completed plan step (F1): `rounds_since_progress`
        // resets whenever a step closes, so a long plan-driven task isn't capped
        // by total rounds — only by getting STUCK on a single step.
        let rounds_since_progress = round.saturating_sub(ls.progress_anchor_round);
        if rounds_since_progress >= max_rounds {
            loop_exit = Some("round_budget_since_last_progress");
            break;
        }
        // Wander-cap: the model has navigated to many sources for the CURRENT step
        // without closing it → it's hopping across walled/SPA pages that won't read.
        // Stop browsing and synthesize from what we already gathered (the forced-
        // synthesis below runs because `final_done` is false; #3a compaction kept the
        // data). `ls.step_evidence` is cleared on every step close, so this counts
        // navigations for the CURRENT step only. Restores 64f08e4d's budget control
        // for the distinct-URL case (cold-context consent walls → wander → burn).
        let mut force_browser_done_this_round: Option<&'static str> = None;
        if ls.browser_used {
            let step_navs = ls
                .step_evidence
                .iter()
                .filter(|e| e.starts_with("browser_navigate"))
                .count();
            if step_navs >= cfg.browser_nav_cap {
                let _ = event_sink
                    .emit(GenerateStreamEvent::Delta {
                        text: "‹‹ACT››⏹️ Enough sources visited — synthesizing from what I \
gathered‹‹/ACT››"
                            .to_string(),
                    })
                    .await;
                if cfg.browser_subturn && has_tool_schema(&ls.tool_schemas, "browser_done") {
                    force_browser_done_this_round = Some("navigation_cap");
                } else {
                    loop_exit = Some("browser_nav_cap_reached");
                    break;
                }
            }
        }
        // Context hygiene: at up to 32 rounds the ls.accumulated snapshots/images
        // would overflow the window and silently truncate the page. Stub all
        // but the latest browser snapshot + the latest screenshot image.
        prune_browser_history(&mut ls.messages, &ls.browser_tool_call_ids);
        // F3: a step was verified last round → collapse its ls.messages into a summary
        // now (safe boundary: all prior tool results are flushed). Keeps a long
        // multi-step turn from overflowing the context window.
        apply_context_compaction_at_round_boundary(
            &mut ls,
            compactor,
            execution_journal,
            round,
            cfg.context_window,
        )
        .await;
        // Fase 1.1: token-budget auto-compaction (the memory-checkpoint path) — independent of
        // plan steps. Fires when the conversation approaches the model's context window, flushing
        // the older span to the memory engine and collapsing it in-context. Same safe round
        // boundary as the step compaction; fail-open (unknown window → no-op) so a turn without a
        // known window keeps exactly today's round-based hygiene.
        execution_journal.checkpoint(crate::LoopCheckpoint::from_state(round, &ls));
        // On the LAST allowed round, forbid tools so the model MUST synthesize
        // a final answer from what it already gathered — otherwise it can burn
        // every round on tool calls and end with no answer ("limite di passi").
        // On the LAST allowed round, OMIT tools entirely (do not rely on
        // tool_choice:"none" — minimax-via-Ollama ignores it and keeps calling
        // tools, so the loop never synthesizes and ends with "limite di passi").
        // Omitting the tools field forces a text answer.
        // Measure the "final round" from the LAST PROGRESS (per-step budget), NOT the
        // total round count — same basis as the `break` above. Using total `round` here
        // was a bug: a long but STEADILY PROGRESSING plan (e.g. a 5-step browse) hit
        // is_final_round at round 32 and got force-synthesized MID-PLAN, ending the turn
        // incomplete so the user had to keep typing "continua". Now the turn only forces a
        // final answer when a SINGLE step stalls for the whole budget (or the 600-round
        // hard ceiling) — so the harness drives the task end-to-end on its own.
        let is_final_round = rounds_since_progress + 1 >= max_rounds;
        if force_browser_done_this_round.is_none()
            && cfg.browser_subturn
            && is_final_round
            && turn_used_tools
            && has_tool_schema(&ls.tool_schemas, "browser_done")
        {
            force_browser_done_this_round = Some("final_round");
        }
        // Final round: tools are omitted so the model MUST answer in text. Without an
        // explicit directive a model that was mid-browse writes a TRANSITION note
        // ("now I'll compose the briefing", "I'll update the plan") instead of the
        // deliverable, and that narration becomes the final answer (observed on Mondiali:
        // ~15 pages browsed, budget spent, ended on "compongo il briefing" with 0/4 steps).
        // Tell it to OUTPUT the finished deliverable from what it already gathered. The
        // separate forced-synthesis (`!final_done` path below) covers the case where the
        // model instead keeps calling tools until the budget breaks the loop.
        if let Some(reason) = force_browser_done_this_round {
            let content = match reason {
                "navigation_cap" => {
                    "You have reached the navigation/source cap for this browser task. Do NOT \
navigate, click, scroll, or take another snapshot. Call browser_done NOW using the concrete facts \
already visible in the browser observations. For a list result contract, fill items with one object \
per result and include every required field. If fewer required items are genuinely available, use \
status=\"partial\" and list the missing fields."
                }
                _ => {
                    "This is your FINAL browser-contract step. Do NOT write free-form prose and do \
NOT call any browser tool except browser_done. Call browser_done NOW using the concrete facts already \
visible in the browser observations. For a list result contract, fill items with one object per \
result and include every required field. If fewer required items are genuinely available, use \
status=\"partial\" and list the missing fields."
                }
            };
            ls.messages
                .push(serde_json::json!({ "role": "user", "content": content }));
        } else if is_final_round && turn_used_tools {
            ls.messages.push(serde_json::json!({
                "role": "user",
                "content": "This is your FINAL step — no more tools or browsing are \
available. Write the COMPLETE deliverable NOW from everything you already gathered: the full \
answer with the ACTUAL data (real rows/values/tables, every option), in the user's language. Do \
NOT narrate what you are about to do (no \"now I'll compose…\", no \"I'll update the plan…\") and \
do NOT promise further work — output the finished result itself. If some data is genuinely \
missing, give what you have and note the gap in one short line.",
            }));
        }
        // The per-round model call now lives in the engine::ModelClient impl (ADR 0024):
        // HTTP, retry/backoff, provider fallback, and the OpenAI/Ollama stream collectors are
        // owned there. A mid-round provider swap comes back via ProviderBinding so the next
        // rounds use the effective provider.
        let browser_done_tool_schemas;
        let tools_for_round = if force_browser_done_this_round.is_some() {
            browser_done_tool_schemas = only_tool_schema(&ls.tool_schemas, "browser_done");
            browser_done_tool_schemas.as_slice()
        } else {
            ls.tool_schemas.as_slice()
        };
        let model_is_final_round = is_final_round && force_browser_done_this_round.is_none();
        let forced_tool_for_round = if force_browser_done_this_round.is_some() {
            Some("browser_done")
        } else if round == 0 {
            cfg.forced_tool.as_deref()
        } else {
            None
        };

        execution_journal.record(AgentExecutionEvent::PromptSnapshot {
            round,
            snapshot: crate::execution_journal::build_prompt_snapshot_with_packets(
                &ls.provider.model,
                &ls.provider.base_url,
                &ls.messages,
                tools_for_round,
                model_is_final_round,
                forced_tool_for_round,
                &ls.prompt_packets,
            ),
        });
        let mut round_usage = usage_context.clone();
        round_usage.call_id = format!("{}:round:{round}", usage_context.call_id);
        round_usage.round = u32::try_from(round).ok();
        let round_call = crate::ModelCall {
            base_url: &ls.provider.base_url,
            model: &ls.provider.model,
            api_key: ls.provider.api_key.as_deref(),
            messages: &ls.messages,
            tools: tools_for_round,
            temperature,
            is_final_round: model_is_final_round,
            forced_tool: forced_tool_for_round,
            usage: &round_usage,
        };
        let mut control_during_model = None;
        let out = tokio::select! {
            out = model_client.generate(&round_call, &|_tok| {}) => Some(out),
            control = wait_for_interrupting_control(model_client) => {
                apply_turn_control(model_client, &mut ls.messages, &control);
                steering_to_complete.push(control.steering_id);
                execution_journal.checkpoint(crate::LoopCheckpoint::from_state(round, &ls));
                control_during_model = Some(control);
                None
            }
        };
        if let Some(control) = control_during_model {
            match control.disposition {
                TurnControlDisposition::ReplanCurrentWork => continue 'rounds,
                TurnControlDisposition::FinalizeWithCurrentEvidence => {
                    loop_exit = Some("steering_finalize_post_model");
                    break 'rounds;
                }
                TurnControlDisposition::NeedsClarification => {
                    awaiting_envelope = Some(HitlEnvelope {
                        kind: HitlKind::Clarify,
                        hold_policy: crate::hitl::HoldPolicy::Free,
                        payload: serde_json::json!({}),
                        source_marker: "steering_clarify".into(),
                    });
                    loop_exit = Some("steering_needs_clarification_post_model");
                    break 'rounds;
                }
                TurnControlDisposition::CancelCurrentWork => {
                    final_done = true;
                    loop_exit = Some("steering_cancel_post_model");
                    break 'rounds;
                }
                TurnControlDisposition::ContinueCurrentWork => continue 'rounds,
            }
        }
        let Some(out) = out else {
            continue 'rounds;
        };
        let (message, round_finish_reason) = match out {
            Ok(o) => {
                // Adopt any mid-turn fallback swap for the remaining rounds.
                ls.provider = o.provider;
                (o.message, o.finish_reason)
            }
            // Upstream and transport failures end the loop; a visible final answer may still be
            // recovered by the post-loop synthesis from accumulated turn prose.
            Err(crate::ModelCallError::Upstream(message)) => {
                protocol_failure =
                    Some(local_first_execution_protocol::ExecutionFailure::permanent(
                        "model_upstream_error",
                        message,
                    ));
                loop_exit = Some("model_upstream_error");
                break;
            }
            Err(crate::ModelCallError::Transport(message)) => {
                protocol_failure =
                    Some(local_first_execution_protocol::ExecutionFailure::permanent(
                        "model_transport_error",
                        message,
                    ));
                loop_exit = Some("model_transport_error");
                break;
            }
            // The model can't see the images it was sent. This is recoverable — but ONLY as a replay of
            // the whole turn from a re-seeded conversation, so it is recoverable only while the turn is
            // still inert. Once a tool has run, replaying would run it twice; at that point the
            // rejection is just a fatal upstream error like any other.
            Err(crate::ModelCallError::ImageUnsupported(reason)) => {
                if turn_used_tools {
                    loop_exit = Some("image_unsupported_after_tools");
                    break;
                }
                // Return with NOTHING emitted and nothing committed: no Done, no answer, no memory. The
                // gateway replaces the images with a vision model's description and calls us again, and
                // the user never sees that this attempt happened.
                return crate::TurnOutcome {
                    image_rejection: Some(reason),
                    ..Default::default()
                };
            }
        };
        let raw_content = message
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let mut tool_calls = message
            .get("tool_calls")
            .and_then(|value| value.as_array())
            .filter(|calls| !calls.is_empty())
            .cloned()
            .or_else(|| {
                // Fallback: some models (e.g. minimax via Ollama) emit tool calls
                // as TEXT in their native template instead of the structured
                // tool_calls field. Parse those so the loop still progresses — but
                // NOT on the final round, which must synthesize a text answer.
                if is_final_round {
                    return None;
                }
                let known: Vec<String> = ls
                    .tool_schemas
                    .iter()
                    .filter_map(|t| {
                        t.get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .map(String::from)
                    })
                    .collect();
                let parsed = model_normalize::parse_text_tool_calls(&raw_content, &known);
                if parsed.is_empty() {
                    None
                } else {
                    Some(model_normalize::synthesize_tool_calls(round, parsed))
                }
            });
        if let Some(calls) = tool_calls.as_mut() {
            normalize_tool_call_ids_for_round(round, calls);
        }

        execution_journal.record(AgentExecutionEvent::ModelResponse {
            round,
            finish_reason: round_finish_reason.clone(),
            content_chars: raw_content.chars().count(),
            tool_calls: tool_calls.as_ref().map_or(0, Vec::len),
        });

        // Turn trace: record this round's outcome (finish_reason + tools chosen) before `tool_calls` is
        // consumed below. Observability only — reads state, never alters it.
        turn_trace.record(crate::turn_trace::TurnEvent::Round {
            round,
            finish_reason: round_finish_reason.clone().unwrap_or_default(),
            tool_calls: tool_calls
                .as_ref()
                .map(|calls| {
                    calls
                        .iter()
                        .filter_map(|c| {
                            c.get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(|n| n.as_str())
                                .map(String::from)
                        })
                        .collect()
                })
                .unwrap_or_default(),
            content_delta_len: raw_content.chars().count(),
        });

        if let Some(calls) = tool_calls {
            plan_nudges = 0; // the model is acting again → reset the stop-nudge cap
            turn_used_tools = true; // slice 2.5: acted → eligible for plan-bootstrap on a premature stop
            // No-progress guard: if this round's tool calls are IDENTICAL to the
            // previous round's, the agent is stuck repeating itself → stop after a
            // couple of repeats and let the forced synthesis answer.
            let round_sig = calls
                .iter()
                .map(|c| {
                    let f = c.get("function");
                    let name = f
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("");
                    let args = f
                        .and_then(|f| f.get("arguments"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("");
                    format!("{name}:{args}")
                })
                .collect::<Vec<_>>()
                .join("|");
            if !round_sig.is_empty() && round_sig == ls.last_round_sig {
                ls.repeat_count += 1;
                // Repetition is a signal to CHANGE APPROACH, not a reason to give up. The old code
                // broke out of the turn on the second identical round, so a model stuck on one form
                // field lost the whole task instead of being told what it was doing wrong — and the
                // user saw a truncated answer with no explanation. Tell it first, naming the exact
                // repeated call, and only stop if it repeats again AFTER being told (that is a model
                // that cannot self-correct, not one that merely needed a hint).
                if ls.repeat_count == REPEAT_NUDGE_AT {
                    let repeated = round_sig.chars().take(200).collect::<String>();
                    ls.messages.push(serde_json::json!({
                        "role": "system",
                        "content": format!(
                            "You have now issued the identical call(s) {} times with no progress: {repeated}. \
Repeating it again will not work. CHANGE APPROACH for this specific step: target a different \
element/ref, use a different action kind, or reach the same goal another way (e.g. read the page \
again to find the right control). Keep working on the task — do not stop and do not start over.",
                            ls.repeat_count + 1
                        ),
                    }));
                    let _ = event_sink
                        .emit(GenerateStreamEvent::Delta {
                            text: "‹‹ACT››🔁 Same action repeated: telling the model to change approach‹‹/ACT››"
                                .to_string(),
                        })
                        .await;
                } else if ls.repeat_count >= REPEAT_STOP_AT {
                    let _ = event_sink
                        .emit(GenerateStreamEvent::Delta {
                            text:
                                "‹‹ACT››⏹️ Same actions repeated after a change-approach hint: stopping and summarizing‹‹/ACT››"
                                    .to_string(),
                        })
                        .await;
                    loop_exit = Some("repeated_action_after_change_approach_hint");
                    break;
                }
            } else {
                ls.repeat_count = 0;
                ls.last_round_sig = round_sig;
            }
            // Echo the assistant's tool-call turn, then append each tool result.
            // Keep provider-required tool_calls, but never preserve planning prose
            // from the same round as model-visible conversation history.
            ls.messages.push(serde_json::json!({
                "role": "assistant",
                "content": "",
                "tool_calls": calls,
            }));
            // Set when a write tool needs confirmation: we stop the loop and let
            // the user run it from the card instead of looping/hallucinating.
            let mut pending_confirm = false;
            let mut stop_for_no_progress = false;
            let mut nudge_no_progress = false;
            let mut delegated_browse_no_progress = false;
            let mut control_after_tools = None;
            let mut terminal_route_block = false;
            // Set when the plan-approval gate defers the pending work tools this round;
            // the post-loop check below `continue`s to a fresh round so the model can
            // propose a plan (PLAN_PROPOSE) and stop for approval.
            let mut plan_approval_injected = false;
            // Bundle the turn-level state the dispatch loop touches into `ctx` so
            // the loop body addresses it via `ctx.<field>` (the seam a later refactor
            // extracts into a function). Built once per round; its block ends right
            // after the dispatch loop so the borrows release before the post-loop
            // reads (screenshot push, `if pending_confirm`) touch the raw locals.
            {
                for (idx, call) in calls.iter().enumerate() {
                    // Parity-harness snapshots (see `tool_trace_dump`): capture the
                    // pre-dispatch state so the record built after the tool push can
                    // measure the deltas. Zero cost beyond three cheap reads when the
                    // dump is disarmed; the record itself is fully gated below.
                    // `pc_before` lets us attribute `pending_confirm` to the call that
                    // RAISED it (the flag lives outside the loop and is never reset).
                    let acc_before = ls.accumulated.len();
                    let msgs_before = ls.messages.len();
                    let pc_before = pending_confirm;
                    let name = call
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("");
                    let args_raw = call
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("{}");
                    let call_id = call
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or("")
                        .to_string();
                    // Recover a typo'd native browser tool name (e.g. "browser_tavigate")
                    // BEFORE dispatch, so it matches the native browser arm instead of
                    // falling through to the Composio catch-all (404 → model loops). The
                    // `browser_` namespace is reserved for native tools. No-op for any
                    // non-browser tool name.
                    let name = match resolve_browser_chat_tool_name(name) {
                        Some(canonical) => canonical,
                        None => name,
                    };

                    // Plan-approval gate (ADR 0021, single-loop): before dispatching a
                    // “work” tool for a complex request, require the model to propose a
                    // plan for the user to approve (PLAN_PROPOSE) instead of executing
                    // unilaterally. Introspective/planning tools are exempt — the gate
                    // must never fire for `update_plan`, `recall_memory`, etc.
                    if !plan_approval_requested
                        && !is_introspective_tool(name)
                        && plan_value_steps(&ls.plan).is_empty()
                        && request_is_complex(&memory_user_message, &calls)
                    {
                        plan_approval_requested = true;
                        turn_trace.record(crate::turn_trace::TurnEvent::Nudge {
                            reason: "plan_approval_before_act".into(),
                            next_step: String::new(),
                        });
                        // Defer the pending work tools (this call and any remaining) and
                        // ask the model to propose a plan for approval. Each deferred call
                        // still gets a tool message so the OpenAI-compat history stays
                        // consistent (every assistant tool_call is followed by a tool msg).
                        for skipped in calls.iter().skip(idx) {
                            ls.messages.push(serde_json::json!({
                                "role": "tool",
                                "tool_call_id": skipped
                                    .get("id")
                                    .and_then(|id| id.as_str())
                                    .unwrap_or(""),
                                "content": "Tool call deferred: propose a plan for approval first.",
                            }));
                        }
                        ls.messages.push(serde_json::json!({
                            "role": "user",
                            "content": "Before executing this multi-step work, propose a plan for the user to approve. Emit ‹‹PLAN_PROPOSE››{\"summary\":\"one-line summary\",\"steps\":[\"step 1\",\"step 2\"]}‹‹/PLAN_PROPOSE›› with your proposed steps and STOP. Do NOT execute any tools until the user approves or edits the plan.",
                        }));
                        let _ = event_sink
                            .emit(GenerateStreamEvent::Delta {
                                text: "‹‹ACT››▶ Propongo un piano per l'approvazione‹‹/ACT››"
                                    .to_string(),
                            })
                            .await;
                        plan_approval_injected = true;
                        break;
                    }

                    if let Some(blocked) = turn_policy.route_blocked(name) {
                        // Parity harness: the blocked arm pushes a tool message then
                        // `continue`s, jumping over the normal record block below. The
                        // upcoming extraction handles this arm specially, so it MUST be
                        // visible to the oracle — emit a record here with `blocked:true`.
                        // Compute the `blocked`-derived fingerprint fields BEFORE the
                        // push moves `blocked` into the message. Fully gated → no cost
                        // when disarmed.
                        let blocked_trace = if crate::trace::dump_enabled() {
                            let normalized = crate::trace::normalize(&blocked);
                            Some((
                                crate::trace::hash_hex(&crate::trace::normalize(args_raw)),
                                crate::trace::hash_hex(&normalized),
                                blocked.chars().count(),
                                normalized.chars().take(120).collect::<String>(),
                            ))
                        } else {
                            None
                        };
                        ls.messages.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": call_id,
                            "content": blocked,
                        }));
                        if let Some((args_hash, result_hash, result_len, result_head)) =
                            blocked_trace
                        {
                            let rec = crate::trace::ToolTraceRecord {
                                round,
                                idx,
                                name: name.to_string(),
                                args_hash,
                                result_hash,
                                result_len,
                                result_head,
                                acc_delta_len: ls.accumulated.len().saturating_sub(acc_before),
                                acc_markers: crate::trace::extract_markers(
                                    acc_before,
                                    &ls.accumulated,
                                ),
                                pending_confirm_raised: pending_confirm && !pc_before,
                                msgs_pushed: ls.messages.len().saturating_sub(msgs_before),
                                blocked: true,
                                browser_image_set: ls.pending_browser_image.is_some(),
                            };
                            if let Some(dir) = trace_dir.as_deref() {
                                crate::trace::append(dir, &rec);
                            }
                        }
                        if turn_policy.route_block_ends_turn() {
                            terminal_route_block = true;
                            break;
                        }
                        continue;
                    }

                    // Record consequential actions (any domain) for decision memory.
                    if ls.tool_trace.len() < 20 {
                        if let Some(line) = summarize_tool_action(name, args_raw) {
                            ls.tool_trace.push(line);
                        } else if let Some(line) =
                            connected_capability_execution_trace_line(name, catalog_index)
                        {
                            ls.tool_trace.push(line);
                        } else if composio_writes.contains(name) {
                            // A write on a connected service (Composio/MCP).
                            ls.tool_trace
                                .push(format!("capability execution connector:{name}"));
                        }
                    }

                    execution_journal.record(AgentExecutionEvent::ToolCallStarted {
                        round,
                        call_id: call_id.clone(),
                        name: name.to_string(),
                    });
                    let (result, tool_effects, interrupted, delegated_browse_timed_out) =
                        if is_browser_granular_tool(name) {
                            // Browser: through the BrowserExecutor seam (ADR 0026 / ADR 0025). The executor owns
                            // the browser subsystem's private state (session, snapshots, tab/nav bookkeeping)
                            // and mutates the loop-visible browser fields + provider via `&mut ls`. Produces no
                            // ToolEffects (it mutates directly). Folds into a recursive `browse` at ADR 0025.
                            tokio::select! {
                                outcome = browser_executor.execute_browser(name, args_raw, &call_id, &mut ls) => {
                                    // Browser dispatch uses the same ToolOutcome contract as every other
                                    // capability. Besides machine progress hints it can therefore suspend
                                    // immediately on an uncertain durable effect receipt.
                                    (outcome.result, outcome.effects, None, false)
                                }
                                control = wait_for_interrupting_control(model_client) => {
                                    (
                                        "Tool execution interrupted by validated user steering.".to_string(),
                                        crate::ToolEffects::default(),
                                        Some(control),
                                        false,
                                    )
                                }
                            }
                        } else {
                            // Non-browser: through the CapabilityExecutor seam (ADR 0026). The executor builds
                            // the per-call ChatToolCtx from `&mut ls` + its held read-only context. `args_raw`
                            // (the model's exact JSON string) is passed through unchanged — no round-trip.
                            let delegated_browse = name == "browse";
                            // The `browse` sub-agent call gets the ABSOLUTE cap as a fresh hard deadline
                            // from NOW (this call) — not a cumulative `cap - elapsed` that shrinks as the
                            // turn runs and folds in pre-browse (e.g. curl) time, which starved the browse
                            // on a slow model. The sub-turn's own per-progress stall window
                            // (`config.rs` `max_stall_ms`, reset on success) is the real control; this is a
                            // final backstop on a single browse call.
                            let browser_deadline = tokio::time::sleep(
                                std::time::Duration::from_millis(cfg.browser_budget.max_elapsed_ms),
                            );
                            tokio::pin!(browser_deadline);
                            tokio::select! {
                                biased;
                                control = wait_for_interrupting_control(model_client) => {
                                    (
                                        "Tool execution interrupted by validated user steering.".to_string(),
                                        crate::ToolEffects::default(),
                                        Some(control),
                                        false,
                                    )
                                }
                                _ = &mut browser_deadline, if delegated_browse => {
                                    (
                                        "found: false\nnote: browse stopped because the turn's browser time budget was exhausted; synthesize from earlier evidence".to_string(),
                                        crate::ToolEffects {
                                            browser_activity_observed: true,
                                            outcome_hint: Some(crate::ToolOutcomeHint::NoProgress),
                                            ..crate::ToolEffects::default()
                                        },
                                        None,
                                        true,
                                    )
                                }
                                outcome = capability_executor.execute_tool(name, args_raw, &call_id, &mut ls) => {
                                    match outcome {
                                        Ok(o) => (o.result, o.effects, None, false),
                                        Err(e) => (e, crate::ToolEffects::default(), None, false),
                                    }
                                }
                            }
                        };
                    if let Some(control) = interrupted {
                        if is_browser_granular_tool(name) {
                            browser_executor.interrupt().await;
                        }
                        execution_journal.record(AgentExecutionEvent::ToolCallCompleted {
                            round,
                            call_id: call_id.clone(),
                            name: name.to_string(),
                            result_chars: result.chars().count(),
                            outcome: "steering_interrupted".to_string(),
                            result_fingerprint: tool_result_fingerprint(&result),
                        });
                        ls.messages.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": call_id,
                            "content": result,
                        }));
                        for skipped in calls.iter().skip(idx + 1) {
                            ls.messages.push(serde_json::json!({
                                "role": "tool",
                                "tool_call_id": skipped.get("id").and_then(|id| id.as_str()).unwrap_or(""),
                                "content": "Tool call skipped because validated user steering changed the active run.",
                            }));
                        }
                        apply_turn_control(model_client, &mut ls.messages, &control);
                        steering_to_complete.push(control.steering_id);
                        execution_journal.checkpoint(crate::LoopCheckpoint::from_state(round, &ls));
                        control_after_tools = Some(control);
                        // Stops iterating THIS round's remaining tool calls, not the turn — the
                        // rounds loop continues and acts on `control_after_tools`. Deliberately not
                        // a `loop_exit` site (see the exit-reason instrumentation above).
                        break;
                    }
                    let outcome = tool_effects
                        .outcome_hint
                        .map(crate::ToolOutcomeHint::as_str)
                        .unwrap_or_else(|| classify_tool_result(&result));
                    let result_fingerprint = tool_result_fingerprint(&result);
                    execution_journal.record(AgentExecutionEvent::ToolCallCompleted {
                        round,
                        call_id: call_id.clone(),
                        name: name.to_string(),
                        result_chars: result.chars().count(),
                        outcome: outcome.to_string(),
                        result_fingerprint,
                    });
                    // Scoped to the browse sub-turn (E2): `browser_done` is that sub-turn's own
                    // completion signal, not a general-purpose terminal. Outside it (`cfg.browser_subturn
                    // == false`) the name is reachable only via hallucination — no non-subturn turn
                    // offers it as a real tool — so it must fall through to normal handling instead of
                    // silently ending the turn on whatever the model made up.
                    if cfg.browser_subturn && name == "browser_done" && !result.trim().is_empty() {
                        memory_answer = result.trim().to_string();
                        ls.messages.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": call_id,
                            "content": result,
                        }));
                        let _ = event_sink
                            .emit(GenerateStreamEvent::Done {
                                text: memory_answer.clone(),
                                metrics: TokenMetrics::zero(),
                                redacted_user_text: None,
                            })
                            .await;
                        visible_answer_delivered = true;
                        final_done = true;
                        loop_exit = Some("browser_done_terminal");
                        break 'rounds;
                    }
                    if is_browser_granular_tool(name) {
                        // MINOR 8: a stale-ref auto-recovery (main.rs) is a real page observation but
                        // NOT progress toward the goal — it comes back as an ordinary `Ok(...)` plain-text
                        // result, which `classify_tool_result` reads as "success" (it isn't the
                        // `{"status":...}` shape), so left unchecked it would reset the counter below and
                        // let a ref-churning SPA loop act→stale→snapshot→act forever. Fold it into the
                        // same stalled bucket as empty/error/blocked so repeated churn still trips
                        // `max_no_progress`.
                        let stalled = is_stale_ref_recovery_result(&result)
                            || matches!(outcome, "empty" | "error" | "blocked" | "no_progress");
                        if stalled {
                            ls.browser_no_progress = ls.browser_no_progress.saturating_add(1);
                            if name == "browser_navigate" {
                                ls.browser_failed_navigations =
                                    ls.browser_failed_navigations.saturating_add(1);
                            }
                        } else {
                            ls.browser_no_progress = 0;
                            // Real browser progress → reset the per-progress stall clock, so the
                            // wall-clock stall window (checked at the top of the loop) measures time
                            // SINCE this success, not since the turn began. Same signal that resets
                            // `browser_no_progress`; keeps the two budgets consistent.
                            last_browser_progress_at = Instant::now();
                            // …and reset the ROUND anchor too. The round cap (`rounds_since_progress
                            // >= max_rounds`) is anchored to `progress_anchor_round`, which otherwise
                            // resets only on plan-frontier progress (a closed step). A browse sub-turn
                            // has no plan, so without this the anchor stays at 0 and the cap counts
                            // TOTAL rounds — cutting off a browse that advances a form field every
                            // round at exactly `browser_max_rounds` while it is still progressing (the
                            // Trenitalia round-9 stall). Browser progress IS progress: a run that keeps
                            // advancing is bounded only by a genuine stall (`stop_reason`), never an
                            // absolute round count.
                            ls.progress_anchor_round = round;
                        }
                    } else {
                        let no_progress_count =
                            ls.observe_tool_outcome(&tool_family(name), outcome);
                        if no_progress_count > 0 {
                            // Task #9: cross-family consecutive failure counter. Each
                            // no-progress tool outcome increments; success or plan progress resets.
                            ls.consecutive_step_failures =
                                ls.consecutive_step_failures.saturating_add(1);
                            if no_progress_count == 2 {
                                nudge_no_progress = true;
                            } else if no_progress_count >= 3 {
                                stop_for_no_progress = true;
                            }
                        }
                    }
                    if matches!(name, "update_plan" | "step_advance") {
                        // Task #9: the model is actively replanning — reset the failure streak
                        // so a fresh plan approach gets a clean slate.
                        ls.consecutive_step_failures = 0;
                        execution_journal.record(AgentExecutionEvent::PlanUpdated {
                            round,
                            source: name.to_string(),
                        });
                    }
                    let browser_activity_observed = tool_effects.browser_activity_observed;
                    let browser_activity_is_progress = browser_activity_observed
                        && !matches!(outcome, "empty" | "error" | "blocked" | "no_progress");
                    let suspend_effect_receipt = tool_effects.suspend_effect_receipt.clone();
                    let capability_runtime_event =
                        capability_runtime_tool_result_payload(name, &tool_effects);
                    // ADR 0024 inc 5d.1b: apply the tool's loop-state effects immediately, before the
                    // loop reads any field they populate (plan, ls.accumulated, …) — net state as inline.
                    ls.apply_effects(&mut pending_confirm, round, tool_effects);
                    if let Some(payload) = capability_runtime_event {
                        let _ = event_sink
                            .emit(GenerateStreamEvent::ToolResult { payload })
                            .await;
                    }
                    if name == "browse" {
                        ls.browser_used = true;
                    }
                    if browser_activity_is_progress {
                        last_browser_progress_at = Instant::now();
                        // Manager-level counterpart of the granular-browser progress reset: only a
                        // delegated browse whose structured outcome is success resets liveness. A
                        // partial/timeout/unavailable browse still marks `browser_used` for projection
                        // and final synthesis, but it must not let the manager wander forever.
                        ls.progress_anchor_round = round;
                    }

                    // Collect source URLs from browser results so the final
                    // answer can carry a deterministic "Fonti" section. The
                    // granular browser_navigate result embeds the visited page URL.
                    if name == "browser_navigate" {
                        // Capture ONLY the VISITED page URL (the first URL in the result,
                        // from "Page opened (URL). Snapshot: …"), not every link scraped
                        // from the page body — otherwise the content snapshot's footer
                        // chrome (Wikipedia donate/edit/history, wikimedia/mediawiki footer
                        // links) lands in the "Sources" footer. The page visited IS the
                        // source. `is_low_value_source_url` stays as a defensive net.
                        if let Some(url) = extract_source_urls(&result).into_iter().next()
                            && !is_low_value_source_url(&url)
                            && !browse_sources.contains(&url)
                        {
                            browse_sources.push(url);
                        }
                    }
                    if name == "browse"
                        && let Some(browse_result) =
                            crate::browse::browse_result_from_manager_text(&result)
                    {
                        for source in &browse_result.sources {
                            if !is_low_value_source_url(source) && !browse_sources.contains(source)
                            {
                                browse_sources.push(source.clone());
                            }
                        }
                        let grounded = !browse_result.sources.is_empty()
                            || !browse_result.items.is_empty()
                            || !browse_result.evidence.is_empty();
                        if browse_result.found
                            && grounded
                            && (!browse_result.answer.trim().is_empty()
                                || !browse_result.items.is_empty())
                        {
                            grounded_browse_result = Some(browse_result);
                        }
                    }
                    if name == "browse" && outcome == "no_progress" {
                        delegated_browse_no_progress = true;
                    }
                    if name == "recall_memory" {
                        ls.pending_vault_reveal_marker = extract_vault_reveal_marker(&result)
                            .or(ls.pending_vault_reveal_marker.take());
                    }
                    // F2: record this tool's outcome as evidence for the current plan
                    // step (the verifier's input). Skip the plan tool itself so the
                    // evidence reflects the actual WORK, not the bookkeeping. Bounded.
                    if !is_plan_bookkeeping_tool(name) {
                        let snippet: String = result.chars().take(400).collect();
                        let evidence_name = evidence_argument_provenance(name, args_raw)
                            .map(|provenance| format!("{name}({provenance})"))
                            .unwrap_or_else(|| name.to_string());
                        ls.step_evidence
                            .push(format!("{evidence_name} → {snippet}"));
                        if ls.step_evidence.len() > 60 {
                            ls.step_evidence.remove(0);
                        }
                        // Structured external-action marker (browser/channel): the F2
                        // deterministic backstop reads these to refuse a `done` claim
                        // whose evidence is only FAILED external actions. Pushed beside
                        // the plain entry, same 60-entry bound.
                        if let Some(marker) = external_action_evidence_marker(name, outcome) {
                            ls.step_evidence.push(marker);
                            if ls.step_evidence.len() > 60 {
                                ls.step_evidence.remove(0);
                            }
                        }
                        // Harness-derived progress: advance the plan frontier when the gathered
                        // evidence VERIFIES the current step (the weak browser model never does).
                        try_advance_frontier_from_evidence(
                            &mut ls,
                            plan_progress,
                            execution_journal,
                            event_sink,
                            &cfg,
                            thread_id,
                            EvidenceVerification {
                                round,
                                force: false,
                            },
                        )
                        .await;
                    }
                    // Parity harness: compute the result-derived fingerprint fields
                    // from `&result` BEFORE the push moves `result` into the message.
                    // Gated on `dump_enabled()` so there is no cost when disarmed.
                    let trace_fields = if crate::trace::dump_enabled() {
                        let normalized = crate::trace::normalize(&result);
                        Some((
                            crate::trace::hash_hex(&crate::trace::normalize(args_raw)),
                            crate::trace::hash_hex(&normalized),
                            result.chars().count(),
                            normalized.chars().take(120).collect::<String>(),
                        ))
                    } else {
                        None
                    };
                    ls.messages.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": result,
                    }));
                    // Build + append the record AFTER the tool push so `msgs_pushed`
                    // counts this push. A browser screenshot pushes a SECOND message
                    // later (outside this loop), which `msgs_pushed` cannot see; that
                    // side effect is instead fingerprinted by `browser_image_set`
                    // below.
                    if let Some((args_hash, result_hash, result_len, result_head)) = trace_fields {
                        let rec = crate::trace::ToolTraceRecord {
                            round,
                            idx,
                            name: name.to_string(),
                            args_hash,
                            result_hash,
                            result_len,
                            result_head,
                            acc_delta_len: ls.accumulated.len().saturating_sub(acc_before),
                            acc_markers: crate::trace::extract_markers(acc_before, &ls.accumulated),
                            pending_confirm_raised: pending_confirm && !pc_before,
                            msgs_pushed: ls.messages.len().saturating_sub(msgs_before),
                            blocked: false,
                            // Set by the browser_screenshot arm (if it ran) to queue a
                            // SECOND message pushed AFTER this loop — invisible to
                            // `msgs_pushed`, so we fingerprint the side effect here.
                            browser_image_set: ls.pending_browser_image.is_some(),
                        };
                        if let Some(dir) = trace_dir.as_deref() {
                            crate::trace::append(dir, &rec);
                        }
                    }
                    if delegated_browse_no_progress {
                        if delegated_browse_timed_out {
                            let elapsed_ms = u64::try_from(turn_started_at.elapsed().as_millis())
                                .unwrap_or(u64::MAX);
                            execution_journal.record(AgentExecutionEvent::BrowserBudgetExceeded {
                                round,
                                reason: "elapsed".to_string(),
                                elapsed_ms,
                                failed_navigations: ls.browser_failed_navigations,
                                no_progress: ls.browser_no_progress,
                            });
                            let _ = event_sink
                                .emit(GenerateStreamEvent::Activity {
                                    text: "browser_budget_exceeded:elapsed".to_string(),
                                })
                                .await;
                            loop_exit = Some("browser_budget_exceeded");
                        } else {
                            let _ = event_sink
                                .emit(GenerateStreamEvent::Activity {
                                    text: "structured_no_progress".to_string(),
                                })
                                .await;
                            loop_exit = Some("structured_no_progress");
                        }
                        break 'rounds;
                    }
                    if let Some(receipt_ref) = suspend_effect_receipt {
                        effect_resolution_receipt = Some(receipt_ref);
                        loop_exit = Some("effect_resolution_required");
                        break 'rounds;
                    }
                }
            } // end ctx scope → borrows freed before the post-loop reads below
            if plan_approval_injected {
                // The plan-approval gate deferred the work tools and asked the model to
                // propose a plan. Skip the post-loop synthesis and go straight to a fresh
                // round so the model can emit PLAN_PROPOSE and stop for approval.
                continue 'rounds;
            }
            if terminal_route_block {
                loop_exit = Some("workflow_result_ready_for_synthesis");
                break 'rounds;
            }
            if let Some(control) = control_after_tools {
                match control.disposition {
                    TurnControlDisposition::ReplanCurrentWork => continue 'rounds,
                    TurnControlDisposition::FinalizeWithCurrentEvidence => {
                        loop_exit = Some("steering_finalize_after_tools");
                        break 'rounds;
                    }
                    TurnControlDisposition::NeedsClarification => {
                        awaiting_envelope = Some(HitlEnvelope {
                            kind: HitlKind::Clarify,
                            hold_policy: crate::hitl::HoldPolicy::Free,
                            payload: serde_json::json!({}),
                            source_marker: "steering_clarify".into(),
                        });
                        loop_exit = Some("steering_needs_clarification_after_tools");
                        break 'rounds;
                    }
                    TurnControlDisposition::CancelCurrentWork => {
                        final_done = true;
                        loop_exit = Some("steering_cancel_after_tools");
                        break 'rounds;
                    }
                    TurnControlDisposition::ContinueCurrentWork => continue 'rounds,
                }
            }
            if nudge_no_progress {
                ls.messages.push(serde_json::json!({
                    "role": "system",
                    "content": "Two consecutive tools in the same family produced no usable progress. Change strategy and do not repeat that tool family unless new evidence makes it necessary."
                }));
            }
            // A browser screenshot this round → feed the image to the (vision)
            // model as a SEPARATE user message. It MUST come AFTER every tool
            // result of this round (OpenAI-compat requires each assistant
            // tool_call to be immediately followed by its tool message; the
            // image cannot sit between them).
            if let Some(dataurl) = ls.pending_browser_image.take() {
                // Send the image ONLY to a vision-capable model. Skip ONLY when /api/show
                // confidently reports no `vision` capability (undetected/cloud → send, as
                // today); a non-vision model would otherwise error on the image part — feed
                // it a text note so it falls back to the page's text snapshot.
                let vision_capable =
                    turn_policy.supports_vision(&ls.provider.base_url, &ls.provider.model);
                if vision_capable {
                    ls.messages.push(serde_json::json!({
                        "role": "user",
                        "content": [
                            { "type": "text", "text": "Screenshot of the current page:" },
                            { "type": "image_url", "image_url": { "url": dataurl } }
                        ],
                    }));
                } else {
                    ls.messages.push(serde_json::json!({
                        "role": "user",
                        "content": "(A screenshot was captured, but this model cannot see images — rely on the page's TEXT snapshot instead.)",
                    }));
                }
            }
            if pending_confirm {
                // A write is awaiting the user's confirmation card — end the turn
                // here (no synthesis, no further tool rounds). Card-only messages
                // have no visible prose after strip; still deliver so the gateway
                // can park on actionable markers (Turn Contract).
                let final_text = append_vault_reveal_marker_if_missing(
                    collapse_plan_markers(&ls.accumulated),
                    ls.pending_vault_reveal_marker.as_deref(),
                );
                if awaiting_envelope.is_none() {
                    awaiting_envelope = crate::hitl::hitl_envelopes_from_text(&final_text)
                        .into_iter()
                        .find(|env| !env.is_free())
                        .or_else(|| {
                            crate::hitl::hitl_envelopes_from_text(&final_text)
                                .into_iter()
                                .next()
                        });
                }
                if visible_answer(&final_text).is_some() || text_awaits_user(&final_text) {
                    let _ = event_sink
                        .emit(GenerateStreamEvent::Done {
                            text: final_text.clone(),
                            metrics: TokenMetrics::zero(),
                            redacted_user_text: None,
                        })
                        .await;
                    memory_answer = final_text;
                    visible_answer_delivered = true;
                    final_done = true;
                }
                loop_exit = Some("pending_user_confirmation");
                break;
            }
            // Task #8/#9: Replan nudge on stall / consecutive failures.
            // When the same-family stall guard fires (3+ consecutive no-progress in one
            // tool family) OR the cross-family consecutive failure counter exceeds 2,
            // inject a replan directive and CONTINUE the loop instead of stopping. The
            // model gets one chance to revise the plan; if it fails again the
            // `replan_injected_this_turn` flag prevents a second injection and the
            // existing stop_for_no_progress break fires.
            if !replan_injected_this_turn
                && !is_final_round
                && (stop_for_no_progress || ls.consecutive_step_failures > 2)
            {
                replan_injected_this_turn = true;
                let replan_message = if stop_for_no_progress {
                    let step_title =
                        crate::plan::plan_next_open(&crate::plan::plan_value_steps(&ls.plan))
                            .unwrap_or_else(|| "current step".to_string());
                    format!(
                        "Step \u{00ab}{step_title}\u{00bb} is blocked after multiple attempts with \
                         no progress. Revise your plan using `update_plan`: remove, replace, or \
                         break down the blocked step into smaller sub-steps. Then continue with \
                         the revised plan."
                    )
                } else {
                    let n = ls.consecutive_step_failures;
                    format!(
                        "You have failed {n} consecutive steps. The current plan approach is \
                         not working. Generate a fundamentally different approach using \
                         `update_plan`. Consider: simpler steps, different tools, or asking the \
                         user for clarification."
                    )
                };
                turn_trace.record(crate::turn_trace::TurnEvent::Nudge {
                    reason: "replan_on_stall".into(),
                    next_step: String::new(),
                });
                ls.messages.push(serde_json::json!({
                    "role": "system",
                    "content": replan_message,
                }));
                let _ = event_sink
                    .emit(GenerateStreamEvent::Delta {
                        text: "\u{2039}\u{2039}ACT\u{203a}\u{203a}\u{1f504} Replan: asking the model to revise \
                               the plan\u{2039}\u{2039}/ACT\u{203a}\u{203a}"
                            .to_string(),
                    })
                    .await;
                continue 'rounds;
            }
            if stop_for_no_progress {
                let _ = event_sink
                    .emit(GenerateStreamEvent::Delta {
                        text: "‹‹ACT››⏹️ Nessun progresso dopo più tentativi: cambio strategia e sintetizzo‹‹/ACT››"
                            .to_string(),
                    })
                    .await;
                execution_journal.record(AgentExecutionEvent::ForcedSynthesis {
                    round: Some(round),
                    reason: "structured_no_progress".to_string(),
                });
                loop_exit = Some("structured_no_progress");
                break;
            }
            continue;
        }

        // No tool call → normally the final answer. Sanitize any leaked model
        // control tokens (e.g. minimax `]<]minimax[>[` / `<tool_call>` text) so
        // the user never sees raw template markup.
        let content = model_normalize::sanitize_model_text(
            message
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or(""),
        );
        // Turn Contract chokepoint: one classifier — Await / NudgeEmit / NotHitl.
        // Prose never enters AwaitingUser; only a structured HitlEnvelope does.
        let combined_for_hitl = format!("{}{}", ls.accumulated, content);
        let hitl_class = match classify_no_tools_stop(&content) {
            NoToolsClassification::NotHitl => classify_no_tools_stop(&combined_for_hitl),
            other => other,
        };
        match hitl_class {
            NoToolsClassification::Await(envelope) => {
                if let Some(resolved) = cfg.resolved_hitl.as_ref()
                    && resolved.reopens(&envelope)
                {
                    if !is_final_round && resolved_hitl_nudges < MAX_RESOLVED_HITL_NUDGES {
                        resolved_hitl_nudges += 1;
                        turn_trace.record(crate::turn_trace::TurnEvent::Nudge {
                            reason: "resolved_hitl_wait_reopened".into(),
                            next_step: String::new(),
                        });
                        if !content.trim().is_empty() {
                            ls.messages.push(
                                serde_json::json!({ "role": "assistant", "content": content }),
                            );
                        }
                        ls.messages.push(serde_json::json!({
                            "role": "user",
                            "content": format!(
                                "[RUNTIME PROTOCOL REJECTION]\nThe HITL wait you just emitted is already resolved with «{}». Do NOT emit or ask that wait again. Apply the accepted resolution and continue the same open work now. If no work remains, provide the final answer. Only create a different HITL wait for a genuinely new unresolved condition.",
                                resolved.resolution.trim(),
                            )
                        }));
                        continue;
                    }
                    protocol_failure =
                        Some(local_first_execution_protocol::ExecutionFailure::permanent(
                            "resolved_hitl_reopened",
                            "The model repeatedly reopened a human wait that was already resolved",
                        ));
                    loop_exit = Some("resolved_hitl_reopened");
                    final_done = true;
                    break;
                }
                ls.accumulated.push_str(&content);
                let final_answer = append_vault_reveal_marker_if_missing(
                    collapse_plan_markers(&ls.accumulated),
                    ls.pending_vault_reveal_marker.as_deref(),
                );
                // Do NOT reconcile open plan steps to done: the person has not answered yet.
                memory_answer = final_answer.clone();
                awaiting_envelope = Some(envelope);
                let _ = event_sink
                    .emit(GenerateStreamEvent::Done {
                        text: final_answer,
                        metrics: TokenMetrics::zero(),
                        redacted_user_text: None,
                    })
                    .await;
                visible_answer_delivered = true;
                final_done = true;
                loop_exit = Some("awaiting_user");
                break;
            }
            NoToolsClassification::NudgeEmit(HitlKind::Choice)
                if !is_final_round && choices_card_nudges < MAX_CHOICES_CARD_NUDGES =>
            {
                choices_card_nudges += 1;
                turn_trace.record(crate::turn_trace::TurnEvent::Nudge {
                    reason: "prose_closed_choice_needs_choices_card".into(),
                    next_step: String::new(),
                });
                if !content.trim().is_empty() {
                    ls.messages
                        .push(serde_json::json!({ "role": "assistant", "content": content }));
                }
                ls.messages.push(serde_json::json!({
                    "role": "user",
                    "content": "You asked the user to pick among discrete options in prose. \
Emit a CHOICES card NOW — do not only list them in a table. Use this exact shape on its own line:\n\
‹‹CHOICES››{\"question\":\"your question\",\"multi\":false,\"options\":[\"Option A\",\"Option B\"]}‹‹/CHOICES››\n\
(or ‹‹AWAIT_USER››{\"kind\":\"choice\",\"question\":\"…\",\"options\":[…]}‹‹/AWAIT_USER››). \
Reuse the same options you already listed. No tools, no new search, no plan update — only the card."
                }));
                continue;
            }
            NoToolsClassification::NudgeEmit(HitlKind::Clarify)
                if !is_final_round && clarify_card_nudges < MAX_CLARIFY_CARD_NUDGES =>
            {
                clarify_card_nudges += 1;
                turn_trace.record(crate::turn_trace::TurnEvent::Nudge {
                    reason: "prose_field_request_needs_clarify_card".into(),
                    next_step: String::new(),
                });
                if !content.trim().is_empty() {
                    ls.messages
                        .push(serde_json::json!({ "role": "assistant", "content": content }));
                }
                ls.messages.push(serde_json::json!({
                    "role": "user",
                    "content": "You asked the user for free-text details in prose. \
Emit a CLARIFY card NOW so the harness can wait and resume correctly. Use this exact shape \
on its own line:\n\
‹‹CLARIFY››{\"question\":\"what you need\",\"fields\":[\"field1\",\"field2\"]}‹‹/CLARIFY››\n\
(or ‹‹AWAIT_USER››{\"kind\":\"clarify\",\"question\":\"…\",\"fields\":[…]}‹‹/AWAIT_USER››). \
Reuse the same question/fields you already listed. No tools, no new search, no plan update — only the card."
                }));
                continue;
            }
            NoToolsClassification::NudgeEmit(HitlKind::Payment)
                if payment_card_nudges < MAX_PAYMENT_CARD_NUDGES =>
            {
                payment_card_nudges += 1;
                turn_trace.record(crate::turn_trace::TurnEvent::Nudge {
                    reason: "prose_payment_wait_needs_payment_approval_card".into(),
                    next_step: String::new(),
                });
                if !content.trim().is_empty() {
                    ls.messages
                        .push(serde_json::json!({ "role": "assistant", "content": content }));
                }
                ls.messages.push(serde_json::json!({
                    "role": "user",
                    "content": "You mentioned a Payment Approval Card in prose, but no valid \
PAYMENT_APPROVAL card was emitted. Emit the Payment Approval Card NOW on its own line, using \
actual checkout facts you already gathered. Use this exact shape:\n\
‹‹PAYMENT_APPROVAL››{\"snapshot\":{\"approval_id\":\"pay_<uuid>\",\"merchant\":\"merchant name\",\"domain\":\"checkout domain\",\"amount_minor\":12196,\"currency\":\"USD\",\"product_summary\":\"visible products\",\"payment_method_label\":\"No payment method entered\",\"checkout_fingerprint\":\"stable checkout fingerprint\"}}‹‹/PAYMENT_APPROVAL››\n\
Do not press Pay/Submit, do not invent missing checkout facts, and do not claim the card was \
presented unless you emit this marker.",
                }));
                continue;
            }
            NoToolsClassification::NudgeEmit(_) | NoToolsClassification::NotHitl => {
                // Fall through to plan / deliver paths.
            }
        }
        // A synthesis/table/report step produces its evidence in the candidate answer itself, not
        // in a work tool result. Verify that output before nudging the model to repeat it. The
        // provenance label tells the strict judge that this can prove an analytical deliverable but
        // can never prove an external action; command/action steps still require their tool evidence.
        if !is_final_round
            && !plan_value_steps(&ls.plan).is_empty()
            && let Some(answer_evidence) = candidate_answer_evidence(&content)
        {
            let done_before = plan_value_steps(&ls.plan)
                .iter()
                .filter(|step| plan_step_status(step) == "done")
                .count();
            ls.step_evidence.push(answer_evidence);
            if ls.step_evidence.len() > 60 {
                ls.step_evidence.remove(0);
            }
            try_advance_frontier_from_evidence(
                &mut ls,
                plan_progress,
                execution_journal,
                event_sink,
                &cfg,
                thread_id,
                EvidenceVerification { round, force: true },
            )
            .await;
            let done_after = plan_value_steps(&ls.plan)
                .iter()
                .filter(|step| plan_step_status(step) == "done")
                .count();
            if done_after > done_before {
                plan_nudges = 0;
                // Task #9: verified plan progress — reset the consecutive failure streak.
                ls.consecutive_step_failures = 0;
            }
        }
        if !is_final_round && plan_nudges < MAX_PLAN_NUDGES {
            let plan_steps = plan_value_steps(&ls.plan);
            if let Some(step) = plan_next_open(&plan_steps) {
                // F5 over-running guard: when only the LAST step is still open AND the
                // model already wrote a substantial answer, it almost certainly FINISHED
                // the work and merely forgot to mark that step done. The "continue, no
                // summary yet" nudge then drags it PAST a good answer into a degraded or
                // self-contradictory one (the long-horizon regression). Accept the answer
                // instead. When SEVERAL steps remain open the model genuinely stopped
                // early → keep nudging. Failure-safe: when in doubt (short answer or many
                // open steps), we nudge — never a premature stop.
                let open_left = plan_steps
                    .iter()
                    .filter(|s| plan_step_status(s) != "done")
                    .count();
                if should_nudge_for_open_plan(&content, open_left) {
                    plan_nudges += 1;
                    // Turn trace: the harness nudged the model to keep going on the still-open plan.
                    turn_trace.record(crate::turn_trace::TurnEvent::Nudge {
                        reason: "answer_did_not_conclude_plan".into(),
                        next_step: step.clone(),
                    });
                    if !content.trim().is_empty() {
                        ls.messages
                            .push(serde_json::json!({ "role": "assistant", "content": content }));
                    }
                    // DIRECTIVE nudge: name the exact next step and forbid redoing work —
                    // weak-agentic models otherwise re-run the skill / regenerate images
                    // on a vague "continue" instead of advancing to the next step.
                    ls.messages.push(serde_json::json!({
                        "role": "user",
                        "content": format!(
                            "Do NOT stop and do NOT re-run the skill. Your next unfinished plan \
                             step is: «{step}». Do ONLY that step now, reusing the files you \
                             already created (do not regenerate existing images). Mark it done \
                             with update_plan, then continue to the next step until the plan is \
                             complete. No confirmation, no summary yet."
                        ),
                    }));
                    let _ = event_sink
                        .emit(GenerateStreamEvent::Delta {
                            text: format!(
                                "‹‹ACT››▶ Proseguo: {}‹‹/ACT››",
                                step.chars().take(50).collect::<String>()
                            ),
                        })
                        .await;
                    continue;
                }
                // Delivery is not evidence. Keep every still-open step unchanged; only verified
                // tool evidence or an explicitly verified plan update may close it.
            } else if plan_steps.is_empty()
                && turn_used_tools
                && completion_judge
                    .task_appears_incomplete(&memory_user_message, &content)
                    .await
            {
                // Slice 2.5: the model ACTED but stopped WITHOUT ever creating a plan, and the
                // cheap judge says the request is not finished. Bootstrap a plan so F1–F5 take
                // over — the whole long-horizon machinery is gated on a NON-EMPTY plan, so an
                // empty plan silently bypasses it (the gap behind a generic multi-step task
                // stopping early). `make_deck` is exempt: one-call, never enters this loop.
                plan_nudges += 1;
                // Turn trace: the model acted but never planned → the harness bootstraps a plan. No
                // named next step here (the plan is empty by definition on this path).
                turn_trace.record(crate::turn_trace::TurnEvent::Nudge {
                    reason: "stopped_without_plan".into(),
                    next_step: String::new(),
                });
                if !content.trim().is_empty() {
                    ls.messages
                        .push(serde_json::json!({ "role": "assistant", "content": content }));
                }
                ls.messages.push(serde_json::json!({
                    "role": "user",
                    "content": "You stopped, but the request is NOT finished and you never made \
                        a plan. Call update_plan NOW with the COMPLETE list of steps needed to \
                        fully satisfy the request, then do the FIRST unfinished step — reuse \
                        anything you already produced, do not redo work. Mark steps done with \
                        update_plan/step_advance as you go. No summary yet.",
                }));
                let _ = event_sink
                    .emit(GenerateStreamEvent::Delta {
                        text: "‹‹ACT››▶ Pianifico il lavoro rimanente‹‹/ACT››".to_string(),
                    })
                    .await;
                continue;
            }
        }
        // F3-deep: the model is about to finalize but produced NO answer body this round
        // — typically a reasoning model that burned its whole token budget thinking
        // (`finish_reason:length`, empty content) so only a ‹‹REASONING›› trace remains.
        // Committing it would Done an empty / reasoning-only bubble (the "non produce la
        // risposta" report). Recover by breaking WITHOUT `final_done`: the guaranteed
        // forced-synthesis (`!final_done` below) then writes a real answer with a FRESH
        // token budget and an explicit "write the FINAL ANSWER now" directive. `break`
        // leaves the round loop, so the synthesis runs exactly once — no spin, no counter;
        // if it too comes back empty, the turn reports `NoVisibleAnswer` without emitting Done.
        let mut candidate = String::with_capacity(ls.accumulated.len() + content.len());
        candidate.push_str(&ls.accumulated);
        candidate.push_str(&content);
        if visible_answer(&candidate).is_none() {
            if cfg.verbose {
                let fr = round_finish_reason.as_deref().unwrap_or("");
                eprintln!("[answer] empty answer body (finish_reason={fr}) → forced synthesis");
            }
            turn_trace.record(crate::turn_trace::TurnEvent::ForcedSynthesis {
                finish_reason: round_finish_reason.clone().unwrap_or_default(),
            });
            execution_journal.record(AgentExecutionEvent::ForcedSynthesis {
                round: Some(round),
                reason: "empty_visible_answer".to_string(),
            });
            // Keep the reasoning trace in context so the synthesis builds on it.
            if !content.trim().is_empty() {
                ls.messages
                    .push(serde_json::json!({ "role": "assistant", "content": content }));
            }
            loop_exit = Some("empty_visible_answer_forced_synthesis");
            break;
        }
        // The content already streamed LIVE (raw) via collect_openai_stream; here we
        // only accumulate the SANITIZED version, which becomes the authoritative
        // `Done` payload that the frontend uses as the final text (replacing the
        // raw live preview). No second content Delta — that would double it.
        ls.accumulated.push_str(&content);
        if let Some(fonti) = fonti_section(&browse_sources, &ls.accumulated) {
            ls.accumulated.push_str(&fonti);
            let _ = event_sink
                .emit(GenerateStreamEvent::Delta { text: fonti })
                .await;
        }
        // Anti-churn: the live stream carried one ‹‹PLAN›› block per plan tool call;
        // the PERSISTED answer keeps the plan card exactly once (latest state).
        // Reconcile the plan ONE last time on delivery — mark every still-open step done
        // (see plan_steps_reconciled_on_delivery) — and PERSIST it, so the runtime plan is
        // settled → the next turn won't falsely resume a plan this answer already finished.
        let delivered = collapse_plan_markers(&ls.accumulated);
        if visible_answer(&delivered).is_none() {
            loop_exit = Some("no_visible_answer_at_delivery");
            break;
        }
        // Turn trace: open-step count BEFORE the final reconcile (its input), captured so the trace
        // shows how many steps the delivery reconcile swept closed.
        let final_open_before = plan_value_steps(&ls.plan)
            .iter()
            .filter(|s| plan_step_status(s) != "done")
            .count();
        let final_delivered_chars = delivered.trim().chars().count();
        let delivered = match plan_progress.reconcile_on_delivery(&ls.plan, &delivered) {
            Some(reconciled) => {
                let plan_goal = plan_value_goal(&ls.plan);
                plan_progress
                    .persist_plan(thread_id, plan_goal.as_deref(), &reconciled)
                    .await;
                event_sink
                    .emit(GenerateStreamEvent::PlanUpdate {
                        markdown: build_plan_markdown(plan_goal.as_deref(), &reconciled),
                    })
                    .await;
                turn_trace.record(crate::turn_trace::TurnEvent::Reconcile {
                    fired: true,
                    step: String::new(), // the whole plan is reconciled here, not a single named step
                    open_steps: final_open_before,
                    delivered_chars: final_delivered_chars,
                    threshold: crate::plan::MIN_DELIVERED_CHARS_TO_CONCLUDE,
                });
                replace_latest_plan_marker(&delivered, plan_goal.as_deref(), &reconciled)
            }
            None => delivered,
        };
        let final_answer = append_vault_reveal_marker_if_missing(
            delivered,
            ls.pending_vault_reveal_marker.as_deref(),
        );
        if visible_answer(&final_answer).is_some() {
            if model_client.finalization_fence() == crate::FinalizationFence::PendingInput {
                ls.messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": final_answer,
                }));
                ls.accumulated.clear();
                continue;
            }
            memory_answer = final_answer.clone();
            let _ = event_sink
                .emit(GenerateStreamEvent::Done {
                    text: final_answer,
                    metrics: TokenMetrics::zero(),
                    redacted_user_text: None,
                })
                .await;
            visible_answer_delivered = true;
            final_done = true;
        }
        loop_exit = Some("model_stopped_naturally");
        break;
    }

    // The single place that records WHY the loop stopped. `None` is only honest when the round range
    // genuinely ran out; if the loop left early without a reason, someone added a `break` and forgot
    // to name it — say so instead of reporting a plausible-looking exhaustion (the exact failure mode
    // that let a whole class of plan bugs hide behind `unwrap_or("todo")`).
    let exhausted_range = last_round + 1 >= cfg.hard_round_ceiling;
    turn_trace.record(crate::turn_trace::TurnEvent::LoopExit {
        reason: loop_exit
            .unwrap_or(if exhausted_range {
                "round_ceiling_reached"
            } else {
                "uninstrumented_exit"
            })
            .to_string(),
        last_round,
        rounds_since_progress: last_round.saturating_sub(ls.progress_anchor_round),
        browsed: ls.browser_used,
    });

    // Turn end (ALL exit paths converge here: normal answer, pending_confirm, round-budget break,
    // natural exhaustion). The browser executor parks its session warm for the thread's next turn (or
    // stops it for an anonymous chat) and hides the "● LIVE" activity — see `close_session`.
    browser_executor.close_session(ls.browser_used).await;

    // Exhaustion is still a finalization path. Fence it exactly like the
    // in-loop delivery path so a steering instruction that was queued,
    // claimed, or interpreted while the last tool was running cannot be
    // overtaken by the forced no-tools synthesis.
    //
    // Drain interpreted controls (including `continue`) so a steering queued/claimed
    // while the last tool ran is honored before finalization. If the fence stays
    // PendingInput with nothing interpreted (rows pending/claimed the coordinator
    // cannot resolve — e.g. the semantic model is unavailable), park instead of
    // spinning: exit with a non-delivering Parked outcome for coordinator resume.
    let mut park_wait: u32 = 0;
    while !final_done && model_client.finalization_fence() == crate::FinalizationFence::PendingInput
    {
        if let Some(control) = model_client.current_turn_control() {
            apply_turn_control(model_client, &mut ls.messages, &control);
            steering_to_complete.push(control.steering_id);
            if control.disposition == TurnControlDisposition::CancelCurrentWork {
                final_done = true;
            }
            park_wait = 0;
            continue;
        }
        if park_wait >= PARK_WAIT_CYCLES {
            // Never park a turn that already HAS an answer. Parking returns an empty
            // `memory_answer` and deliberately emits no terminal event, because it expects the
            // coordinator to resume the run later. When the work is already done, that trade is
            // pure loss: the user has the full answer on screen, but the bubble keeps spinning
            // forever (no terminal), the answer can be re-streamed by the resume path, and the
            // finished result is discarded. Observed exactly that: a completed train search whose
            // turn parked afterwards and span for 80 minutes with the answer already delivered.
            // An unresolvable steering row is not a reason to throw away finished work — fall
            // through to the normal finalization below and deliver.
            if !memory_answer.trim().is_empty() {
                final_done = true;
                break;
            }
            // Park: capture a resumable checkpoint at the boundary and return without
            // delivering. Do NOT force synthesis, do NOT emit Done. Flush completions for
            // anything already drained/applied earlier in this same turn FIRST — this is the
            // only place those ids would otherwise get acknowledged (the normal completion
            // block below is never reached on this early-return path), and their instructions
            // are already folded into `ls.messages`, which the park checkpoint preserves.
            complete_drained_steering(model_client, &mut steering_to_complete);
            execution_journal.checkpoint(crate::LoopCheckpoint::from_state(last_round, &ls));
            return crate::TurnOutcome {
                stop: crate::TurnStop::SuspendedModel {
                    role: "primary".to_string(),
                },
                memory_answer: String::new(),
                image_rejection: None,
                ..Default::default()
            };
        }
        park_wait += 1;
        // A plain wall-clock tick, deliberately NOT `wait_for_turn_control()`: that seam is
        // specified to return only once a non-`continue` interpreted control appears (the real
        // gateway impl loops internally, sleeping, and never returns on its own otherwise) — if
        // the semantic model is down and the row never gets interpreted, awaiting it here would
        // block this tick forever and `park_wait` would never reach the budget, defeating the
        // park. Ticking on a bounded sleep instead means the budget elapses on wall-clock time
        // regardless of whether interpretation ever happens.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    if !final_done && effect_resolution_receipt.is_none() {
        // Turn Contract: never forced-synthesize over a human wait.
        let awaiting_user = awaiting_envelope.is_some()
            || matches!(
                loop_exit,
                Some("pending_user_confirmation")
                    | Some("awaiting_user")
                    | Some("awaiting_user_choice")
                    | Some("steering_needs_clarification_pre_model")
                    | Some("steering_needs_clarification_post_model")
                    | Some("steering_needs_clarification_after_tools")
            )
            || text_awaits_user(&ls.accumulated)
            || text_awaits_user(&memory_answer);
        if awaiting_user {
            if let Some(ref envelope) = awaiting_envelope {
                memory_answer = ensure_free_hitl_marker_in_text(&memory_answer, envelope);
                if memory_answer.trim().is_empty() && text_awaits_user(&ls.accumulated) {
                    memory_answer = ensure_free_hitl_marker_in_text(
                        &append_vault_reveal_marker_if_missing(
                            collapse_plan_markers(&ls.accumulated),
                            ls.pending_vault_reveal_marker.as_deref(),
                        ),
                        envelope,
                    );
                }
            }
            if !memory_answer.trim().is_empty() || text_awaits_user(&ls.accumulated) {
                if memory_answer.trim().is_empty() {
                    let final_text = append_vault_reveal_marker_if_missing(
                        collapse_plan_markers(&ls.accumulated),
                        ls.pending_vault_reveal_marker.as_deref(),
                    );
                    memory_answer = final_text.clone();
                    let _ = event_sink
                        .emit(GenerateStreamEvent::Done {
                            text: final_text,
                            metrics: TokenMetrics::zero(),
                            redacted_user_text: None,
                        })
                        .await;
                } else if !final_done {
                    // Steering Clarify (etc.): emit Done with the injected Free marker so
                    // gateway persist + UI enter AwaitingUser through the typed envelope.
                    let final_text = memory_answer.clone();
                    let _ = event_sink
                        .emit(GenerateStreamEvent::Done {
                            text: final_text,
                            metrics: TokenMetrics::zero(),
                            redacted_user_text: None,
                        })
                        .await;
                }
                visible_answer_delivered = true;
                final_done = true;
            }
            // Skip forced synthesis — fall through to outcome assembly.
        } else {
            let browser_incomplete = ls.browser_used && browser_incomplete_loop_exit(loop_exit);
            let candidate = if browser_incomplete {
                protocol_failure = Some(browser_incomplete_failure(loop_exit));
                Some(browser_exhausted_fallback_answer(
                    loop_exit,
                    grounded_browse_result.as_ref(),
                    &browse_sources,
                ))
            } else {
                // Turn trace: the loop exited without a committed answer → the guaranteed post-loop synthesis
                // fires. No per-round finish_reason applies on this exhaustion path (the loop broke on the
                // round/nav budget or a transport error); a synthetic marker keeps the event greppable.
                turn_trace.record(crate::turn_trace::TurnEvent::ForcedSynthesis {
                    finish_reason: "post_loop_exhausted".into(),
                });
                execution_journal.record(AgentExecutionEvent::ForcedSynthesis {
                    round: None,
                    reason: "post_loop_exhausted".to_string(),
                });
                // Guaranteed synthesis: the model exhausted the tool rounds without a
                // text answer (it kept calling tools). Force one final NO-TOOLS call so it
                // synthesizes from what it did, instead of dead-ending on "limite di passi".
                // GENERIC across domains (coding, documents, web), not travel-specific.
                ls.messages.push(serde_json::json!({
                "role": "user",
                "content": "No more tools are available. Write the FINAL ANSWER NOW for \
            the user, synthesizing what you did and found in the previous steps: for a coding task \
            say what you created/modified and how it's used/run; for a search report the results with \
            details. Be complete and concrete. If something failed, say so clearly and propose how \
            to proceed."
            }));
                // ADR 0024 inc 5 (P2b): the forced synthesis now goes through the SAME
                // engine::ModelClient seam as the per-round call — `is_final_round: true` (no
                // tools, fresh answer budget) — so it inherits retry/backoff, the mid-turn
                // provider fallback and the OpenAI/Ollama-native collectors instead of the old
                // single inline POST (which could dead-end on one transient failure). The impl
                // streams the answer live via its captured StreamSink, exactly like the loop.
                // A provider swap here is moot (the turn ends right after), and any error leaves
                // the turn without a delivery unless already-accumulated text is visibly answer prose.
                execution_journal.record(AgentExecutionEvent::PromptSnapshot {
                    round: cfg.hard_round_ceiling,
                    snapshot: crate::execution_journal::build_prompt_snapshot_with_packets(
                        &ls.provider.model,
                        &ls.provider.base_url,
                        &ls.messages,
                        &[],
                        true,
                        None,
                        &ls.prompt_packets,
                    ),
                });
                let mut synthesis_usage = usage_context.clone();
                synthesis_usage.call_id = format!("{}:forced_synthesis", usage_context.call_id);
                synthesis_usage.purpose_detail = Some("forced_synthesis".to_string());
                synthesis_usage.round = u32::try_from(cfg.hard_round_ceiling).ok();
                let synth_out = model_client
                    .generate(
                        &crate::ModelCall {
                            base_url: &ls.provider.base_url,
                            model: &ls.provider.model,
                            api_key: ls.provider.api_key.as_deref(),
                            messages: &ls.messages,
                            tools: &[],
                            temperature,
                            is_final_round: true,
                            // No tools offered on the forced-synthesis call, so forcing is moot — kept
                            // `None` for consistency with every other non-main-round call site.
                            forced_tool: None,
                            usage: &synthesis_usage,
                        },
                        &|_tok| {},
                    )
                    .await;
                let synth_text = model_normalize::sanitize_model_text(
                    synth_out
                        .as_ref()
                        .ok()
                        .and_then(|o| o.message.get("content"))
                        .and_then(|c| c.as_str())
                        .unwrap_or(""),
                );
                if let Ok(output) = &synth_out {
                    execution_journal.record(AgentExecutionEvent::ModelResponse {
                        round: cfg.hard_round_ceiling,
                        finish_reason: output.finish_reason.clone(),
                        content_chars: synth_text.chars().count(),
                        tool_calls: 0,
                    });
                }
                // synth_text was already streamed live by the collector; commit it only when it contains
                // visible prose. If it does not, the accumulated turn text gets the same validation.
                if visible_answer(&synth_text).is_some() {
                    let synthesis_blocks = preserved_display_marker_blocks(&synth_text);
                    let accumulated_prefix = preserved_display_marker_blocks(&ls.accumulated)
                        .into_iter()
                        .filter(|block| {
                            !synthesis_blocks.iter().any(|synthesis| synthesis == block)
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    Some(if accumulated_prefix.is_empty() {
                        synth_text
                    } else {
                        format!("{accumulated_prefix}\n{synth_text}")
                    })
                } else if visible_answer(&ls.accumulated).is_some() {
                    Some(ls.accumulated.clone())
                } else if ls.browser_used {
                    Some(browser_exhausted_fallback_answer(
                        loop_exit,
                        grounded_browse_result.as_ref(),
                        &browse_sources,
                    ))
                } else {
                    None
                }
            };
            if let Some(mut final_text) = candidate {
                if let Some(fonti) = fonti_section(&browse_sources, &final_text) {
                    final_text.push_str(&fonti);
                }
                let (hitl_text, terminal_envelope) = finalize_terminal_text_for_hitl(&final_text);
                if let Some(envelope) = terminal_envelope {
                    awaiting_envelope = Some(envelope);
                    let final_text = append_vault_reveal_marker_if_missing(
                        collapse_plan_markers(&hitl_text),
                        ls.pending_vault_reveal_marker.as_deref(),
                    );
                    if visible_answer(&final_text).is_some() || text_awaits_user(&final_text) {
                        memory_answer = final_text.clone();
                        let _ = event_sink
                            .emit(GenerateStreamEvent::Done {
                                text: final_text,
                                metrics: TokenMetrics::zero(),
                                redacted_user_text: None,
                            })
                            .await;
                        visible_answer_delivered = true;
                        final_done = true;
                    }
                    // Human wait owns the turn boundary; do not reconcile plan steps to done.
                } else {
                    // Anti-churn safety net for the accumulated fallback (synthesis normally has no plan
                    // blocks). Reconcile + persist only after a visible delivery candidate exists.
                    let delivered = collapse_plan_markers(&hitl_text);
                    let delivered = match plan_progress.reconcile_on_delivery(&ls.plan, &delivered)
                    {
                        Some(reconciled) => {
                            let plan_goal = plan_value_goal(&ls.plan);
                            plan_progress
                                .persist_plan(thread_id, plan_goal.as_deref(), &reconciled)
                                .await;
                            event_sink
                                .emit(GenerateStreamEvent::PlanUpdate {
                                    markdown: build_plan_markdown(
                                        plan_goal.as_deref(),
                                        &reconciled,
                                    ),
                                })
                                .await;
                            replace_latest_plan_marker(
                                &delivered,
                                plan_goal.as_deref(),
                                &reconciled,
                            )
                        }
                        None => delivered,
                    };
                    let final_text = append_vault_reveal_marker_if_missing(
                        delivered,
                        ls.pending_vault_reveal_marker.as_deref(),
                    );
                    if visible_answer(&final_text).is_some() {
                        memory_answer = final_text.clone();
                        let _ = event_sink
                            .emit(GenerateStreamEvent::Done {
                                text: final_text,
                                metrics: TokenMetrics::zero(),
                                redacted_user_text: None,
                            })
                            .await;
                        visible_answer_delivered = true;
                    }
                }
            }
        } // end else (!awaiting_user forced synthesis)
    }
    if final_done || visible_answer_delivered {
        complete_drained_steering(model_client, &mut steering_to_complete);
    }
    // 5.D1c.8: the post-turn tail (memory learn + code-graph refresh) is a GATEWAY concern (AppState /
    // stores / spawn), so it runs in the caller after this returns — driven by the outcome below. The
    // engine's turn ends here.
    if let Some(ref envelope) = awaiting_envelope {
        memory_answer = ensure_free_hitl_marker_in_text(&memory_answer, envelope);
    }
    let stop = match (protocol_failure, effect_resolution_receipt) {
        (Some(failure), _) => crate::TurnStop::Failed { failure },
        (None, Some(receipt_ref)) => crate::TurnStop::SuspendedEffect { receipt_ref },
        (None, None) => crate::outcome::classify_turn_stop(
            visible_answer_delivered || visible_answer(&memory_answer).is_some(),
            awaiting_envelope.as_ref(),
            None,
        ),
    };
    crate::TurnOutcome {
        stop,
        memory_answer,
        tool_actions: ls.tool_trace.join("\n"),
        memory_reads: ls.memory_reads,
        browse_sources,
        // Carry the final runtime plan out for the gateway's turn_trace TurnEnd (observability only).
        final_plan: ls.plan,
        // Reaching here means the turn ran to completion: any image rejection was either recovered by
        // the caller before this attempt, or downgraded to a fatal error above.
        image_rejection: None,
        awaiting_user: awaiting_envelope,
    }
}

async fn apply_context_compaction_at_round_boundary(
    ls: &mut LoopState,
    compactor: &impl ContextCompactor,
    execution_journal: &impl ExecutionJournal,
    round: usize,
    context_window: Option<usize>,
) {
    if ls.pending_compaction {
        ls.pending_compaction = false;
        if compactor
            .compact(&mut ls.messages, &mut ls.step_messages_start)
            .await
        {
            execution_journal.record(AgentExecutionEvent::ContextCompacted {
                round,
                reason: "verified_step_boundary".to_string(),
            });
        }
    }
    if compactor
        .compact_for_budget(&mut ls.messages, context_window, &ls.memory_reads)
        .await
    {
        execution_journal.record(AgentExecutionEvent::ContextCompacted {
            round,
            reason: "context_budget".to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        ModelCall, ModelCallError, ModelRoundOutput, ProviderBinding, ToolEffects, ToolOutcome,
    };
    use crate::events::{LinkedMemoryRead, TurnMemoryReadSet};
    use serde_json::{Value, json};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[test]
    fn load_tools_emits_capability_runtime_payload() {
        let payload = capability_runtime_tool_result_payload(
            "find_capability",
            &ToolEffects {
                load_tools: vec![crate::LoadedTool {
                    key: "mcp__github__list_issues".to_string(),
                    schema: Some(json!({"type": "function"})),
                }],
                arm_sensitive: vec!["financial".to_string()],
                pending_capability: Some("github issue triage".to_string()),
                blocked_capabilities: vec![crate::BlockedCapability {
                    key: "mcp__github__create_issue".to_string(),
                    reason: "approval_required".to_string(),
                }],
                ..ToolEffects::default()
            },
        )
        .expect("capability effects emit metadata");

        assert_eq!(payload["name"], "find_capability");
        assert_eq!(
            payload["capability_runtime"]["loaded_tools"],
            json!(["mcp__github__list_issues"])
        );
        assert_eq!(
            payload["capability_runtime"]["armed_sensitive_domains"],
            json!(["financial"])
        );
        assert_eq!(
            payload["capability_runtime"]["pending_capability"],
            "github issue triage"
        );
        assert_eq!(
            payload["capability_runtime"]["blocked_capabilities"][0],
            json!({"key": "mcp__github__create_issue", "reason": "approval_required"})
        );
    }

    // Minimal seam mocks: the model answers immediately (no tool calls); everything else is a no-op.
    struct AnswerModel;
    impl ModelClient for AnswerModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            on_delta("Final answer.");
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": "Final answer." }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    struct TransportFailureModel;

    impl ModelClient for TransportFailureModel {
        async fn generate(
            &self,
            _call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            Err(ModelCallError::Transport(
                "The model didn't respond (timeout/network). Try again shortly.".to_string(),
            ))
        }
    }

    struct UpstreamFailureModel;

    impl ModelClient for UpstreamFailureModel {
        async fn generate(
            &self,
            _call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            Err(ModelCallError::Upstream(
                "The model provider returned an error. Select a different model.".to_string(),
            ))
        }
    }

    struct StructuredAnswerModel;

    impl ModelClient for StructuredAnswerModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let content = "‹‹PLAN››- [x] Deliver result‹‹/PLAN››\n‹‹ARTIFACT››report.md‹‹/ARTIFACT››\nFinal answer.";
            on_delta(content);
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": content }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[derive(Default)]
    struct ToolCallTextLeakModel {
        calls: AtomicUsize,
        second_round_messages: Mutex<Vec<Value>>,
    }

    impl ModelClient for ToolCallTextLeakModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if index == 0 {
                return Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "Planning text that must not become visible history.",
                        "tool_calls": [{
                            "id": "tool_1",
                            "type": "function",
                            "function": { "name": "recall_memory", "arguments": "{}" },
                        }],
                    }),
                    provider,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                });
            }
            *self.second_round_messages.lock().unwrap() = call.messages.to_vec();
            on_delta("Final answer after tool.");
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": "Final answer after tool." }),
                provider,
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[derive(Default)]
    struct ResolvedChoiceReplayThenAnswerModel {
        calls: AtomicUsize,
    }

    impl ModelClient for ResolvedChoiceReplayThenAnswerModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let replay = self.calls.fetch_add(1, Ordering::SeqCst) == 0;
            let content = if replay {
                r#"‹‹CHOICES››{"question":"Choose the operational option:","multi":false,"options":["ALFA","BETA"]}‹‹/CHOICES››"#
            } else {
                "SCELTA RIPRESA ALFA"
            };
            on_delta(content);
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": content }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[derive(Default)]
    struct ToolThenForcedSynthesisModel {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl ModelClient for ToolThenForcedSynthesisModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let call_index = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if call_index < 2 {
                return Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": format!("tool_{call_index}"),
                            "type": "function",
                            "function": { "name": "make_document", "arguments": "{}" },
                        }],
                    }),
                    provider,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                });
            }
            let content = "‹‹ARTIFACT››report.md‹‹/ARTIFACT››\n‹‹CHOICES››choose a format‹‹/CHOICES››\n‹‹REASONING››synthesis reasoning‹‹/REASONING››\nForced synthesis answer.";
            on_delta(content);
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": content }),
                provider,
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[derive(Default)]
    struct ToolThenForcedSynthesisProseChoiceModel {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl ModelClient for ToolThenForcedSynthesisProseChoiceModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let call_index = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if call_index < 2 {
                return Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": format!("tool_{call_index}"),
                            "type": "function",
                            "function": { "name": "make_document", "arguments": "{}" },
                        }],
                    }),
                    provider,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                });
            }
            let content = concat!(
                "Ho trovato queste opzioni:\n\n",
                "| # | Treno | Prezzo |\n",
                "|---|-------|--------|\n",
                "| 1 | Frecciarossa 9524 | 85,90 EUR |\n",
                "| 2 | Frecciarossa 9310 | 79,90 EUR |\n\n",
                "Per procedere con la prenotazione, quale preferisci?"
            );
            on_delta(content);
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": content }),
                provider,
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[derive(Default)]
    struct StructuredOutputTool {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl CapabilityExecutor for StructuredOutputTool {
        async fn execute_tool(
            &self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            _state: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            let first = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
            Ok(ToolOutcome {
                result: "document created".to_string(),
                effects: ToolEffects {
                    append_output: first.then(|| {
                        "‹‹PLAN››- [x] Deliver result‹‹/PLAN››\n‹‹ARTIFACT››report.md‹‹/ARTIFACT››\n"
                            .to_string()
                    }).into_iter().collect(),
                    ..ToolEffects::default()
                },
            })
        }
    }

    #[derive(Default)]
    struct ReasoningOnlyModel {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl ModelClient for ReasoningOnlyModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let content = "‹‹REASONING››hidden‹‹/REASONING››";
            on_delta(content);
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": content }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }
    struct NoTools;
    impl CapabilityExecutor for NoTools {
        async fn execute_tool(
            &self,
            name: &str,
            _a: &str,
            _c: &str,
            _s: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            Ok(ToolOutcome {
                result: format!("ran {name}"),
                effects: ToolEffects::default(),
            })
        }
    }
    fn browser_outcome(
        result: impl Into<String>,
        hint: crate::contract::ToolOutcomeHint,
    ) -> ToolOutcome {
        ToolOutcome {
            result: result.into(),
            effects: ToolEffects {
                outcome_hint: Some(hint),
                ..ToolEffects::default()
            },
        }
    }
    struct NoBrowser;
    impl BrowserExecutor for NoBrowser {
        async fn execute_browser(
            &mut self,
            _n: &str,
            _a: &str,
            _c: &str,
            _s: &mut LoopState,
        ) -> ToolOutcome {
            browser_outcome(String::new(), crate::contract::ToolOutcomeHint::Success)
        }
        async fn close_session(&mut self, _b: bool) {}
    }

    #[derive(Default)]
    struct UncertainEffectModel {
        calls: AtomicUsize,
        browser: bool,
    }

    impl ModelClient for UncertainEffectModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let call_index = self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(
                call_index, 0,
                "an uncertain effect must suspend before synthesis"
            );
            let tool_name = if self.browser {
                "browser_act"
            } else {
                "connector_send"
            };
            Ok(ModelRoundOutput {
                message: json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "send_1",
                        "type": "function",
                        "function": { "name": tool_name, "arguments": "{}" }
                    }]
                }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("tool_calls".into()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    struct UncertainBrowser;

    impl BrowserExecutor for UncertainBrowser {
        async fn execute_browser(
            &mut self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            _state: &mut LoopState,
        ) -> ToolOutcome {
            ToolOutcome {
                result: "browser outcome requires verification".into(),
                effects: ToolEffects {
                    suspend_effect_receipt: Some(
                        local_first_execution_protocol::EffectReceiptRef::from_store_id(
                            "22222222222222222222222222222222",
                        )
                        .unwrap(),
                    ),
                    outcome_hint: Some(crate::contract::ToolOutcomeHint::NoProgress),
                    ..ToolEffects::default()
                },
            }
        }

        async fn close_session(&mut self, _browser_used: bool) {}
    }

    struct UncertainEffectTool;

    impl CapabilityExecutor for UncertainEffectTool {
        async fn execute_tool(
            &self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            _state: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            Ok(ToolOutcome {
                result: "effect outcome requires verification".into(),
                effects: ToolEffects {
                    suspend_effect_receipt: Some(
                        local_first_execution_protocol::EffectReceiptRef::from_store_id(
                            "11111111111111111111111111111111",
                        )
                        .unwrap(),
                    ),
                    ..ToolEffects::default()
                },
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn uncertain_effect_suspends_without_model_synthesis_or_visible_completion() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "send" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = UncertainEffectModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &model,
            &UncertainEffectTool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "send".into(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert!(matches!(
            outcome.stop,
            crate::TurnStop::SuspendedEffect { .. }
        ));
        assert_eq!(model.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            sink.0
                .lock()
                .unwrap()
                .iter()
                .filter(|event| matches!(event, GenerateStreamEvent::Done { .. }))
                .count(),
            0
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn model_transport_error_is_the_failed_turn_reason_not_no_reply() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "hello" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = TransportFailureModel;
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "hello".into(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let crate::TurnStop::Failed { failure } = outcome.stop else {
            panic!("transport failure must fail the turn");
        };
        assert_eq!(failure.code, "model_transport_error");
        assert_eq!(
            failure.redacted_detail,
            "The model didn't respond (timeout/network). Try again shortly."
        );
        assert_ne!(failure.code, "no_reply");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn model_upstream_error_is_the_failed_turn_reason_not_no_reply() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "hello" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = UpstreamFailureModel;
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "hello".into(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let crate::TurnStop::Failed { failure } = outcome.stop else {
            panic!("upstream failure must fail the turn");
        };
        assert_eq!(failure.code, "model_upstream_error");
        assert_eq!(
            failure.redacted_detail,
            "The model provider returned an error. Select a different model."
        );
        assert_ne!(failure.code, "no_reply");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn uncertain_browser_effect_suspends_without_another_model_round() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "click" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = UncertainEffectModel {
            browser: true,
            ..Default::default()
        };
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = UncertainBrowser;

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "click".into(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert!(matches!(
            outcome.stop,
            crate::TurnStop::SuspendedEffect { .. }
        ));
        assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    }

    #[derive(Default)]
    struct BrowserDoneModel {
        calls: AtomicUsize,
    }

    impl ModelClient for BrowserDoneModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if index == 0 {
                return Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": "call_done",
                            "type": "function",
                            "function": {
                                "name": "browser_done",
                                "arguments": "{\"status\":\"completed\",\"answer\":\"done\",\"items\":[{\"departure\":\"09:05\"}],\"sources\":[\"https://example.test\"],\"evidence\":[\"visible\"]}"
                            }
                        }]
                    }),
                    provider,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                });
            }
            on_delta("forced synthesis should not run");
            Ok(ModelRoundOutput {
                message: json!({"role": "assistant", "content": "forced synthesis should not run"}),
                provider,
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    struct DoneBrowser;

    impl BrowserExecutor for DoneBrowser {
        async fn execute_browser(
            &mut self,
            name: &str,
            _args: &str,
            _call_id: &str,
            state: &mut LoopState,
        ) -> ToolOutcome {
            assert_eq!(name, "browser_done");
            state.browser_used = true;
            browser_outcome("done", crate::contract::ToolOutcomeHint::Success)
        }

        async fn close_session(&mut self, _browser_used: bool) {}
    }

    #[tokio::test(flavor = "current_thread")]
    async fn browser_done_tool_terminates_without_forced_synthesis() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        ls.tool_schemas = vec![json!({
            "type": "function",
            "function": {
                "name": "browser_done",
                "parameters": { "type": "object", "properties": {} }
            }
        })];
        let model = BrowserDoneModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = DoneBrowser;

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(outcome.stop, crate::TurnStop::Completed);
        assert_eq!(outcome.memory_answer, "done");
        assert_eq!(
            model.calls.load(Ordering::SeqCst),
            1,
            "browser_done must not trigger forced synthesis"
        );
        assert_eq!(
            sink.0
                .lock()
                .unwrap()
                .iter()
                .filter(|event| matches!(event, GenerateStreamEvent::Done { .. }))
                .count(),
            1
        );
    }

    #[derive(Default)]
    struct NavCapThenBrowserDoneModel {
        calls: AtomicUsize,
        forced_done_calls: AtomicUsize,
    }

    impl ModelClient for NavCapThenBrowserDoneModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if call.forced_tool == Some("browser_done") {
                self.forced_done_calls.fetch_add(1, Ordering::SeqCst);
                let tool_names = call
                    .tools
                    .iter()
                    .filter_map(|tool| {
                        tool.get("function")
                            .and_then(|function| function.get("name"))
                            .and_then(serde_json::Value::as_str)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(tool_names, vec!["browser_done"]);
                return Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": "call_done",
                            "type": "function",
                            "function": {
                                "name": "browser_done",
                                "arguments": "{\"status\":\"completed\",\"answer\":\"done\",\"items\":[{\"title\":\"A\",\"source\":\"News\",\"summary\":\"One line\"}],\"sources\":[\"https://news.example\"],\"evidence\":[\"snapshot\"]}"
                            }
                        }]
                    }),
                    provider,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                });
            }

            Ok(ModelRoundOutput {
                message: json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": format!("nav_{index}"),
                        "type": "function",
                        "function": {
                            "name": "browser_navigate",
                            "arguments": format!("{{\"url\":\"https://news.example/{index}\"}}")
                        },
                    }],
                }),
                provider,
                finish_reason: Some("tool_calls".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    struct NavCapBrowser;

    impl BrowserExecutor for NavCapBrowser {
        async fn execute_browser(
            &mut self,
            name: &str,
            _args: &str,
            _call_id: &str,
            state: &mut LoopState,
        ) -> ToolOutcome {
            state.browser_used = true;
            match name {
                "browser_navigate" => browser_outcome(
                    "Page opened (https://news.example/0). Snapshot: A source row",
                    crate::contract::ToolOutcomeHint::Success,
                ),
                "browser_done" => {
                    browser_outcome("done", crate::contract::ToolOutcomeHint::Success)
                }
                other => panic!("unexpected browser tool after nav cap: {other}"),
            }
        }

        async fn close_session(&mut self, _browser_used: bool) {}
    }

    #[tokio::test(flavor = "current_thread")]
    async fn browser_subturn_nav_cap_forces_browser_done_only() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        ls.tool_schemas = vec![
            json!({
                "type": "function",
                "function": {
                    "name": "browser_navigate",
                    "parameters": { "type": "object", "properties": {} }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "browser_done",
                    "parameters": { "type": "object", "properties": {} }
                }
            }),
        ];
        let model = NavCapThenBrowserDoneModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NavCapBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.browser_subturn = true;
        turn_cfg.hard_round_ceiling = 6;
        turn_cfg.browser_max_rounds = 6;
        turn_cfg.browser_nav_cap = 1;
        turn_cfg.browser_budget.max_no_progress = 50;
        turn_cfg.browser_budget.max_stall_ms = 600_000;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(outcome.stop, crate::TurnStop::Completed);
        assert_eq!(outcome.memory_answer, "done");
        assert_eq!(model.forced_done_calls.load(Ordering::SeqCst), 1);
    }

    /// E2: `browser_done` is the browse SUB-turn's own completion signal. Outside that sub-turn
    /// (`browser_subturn = false`) the same tool name — reached only via hallucination, since
    /// non-subturn turns never offer it as a real capability — must NOT terminate the turn. The
    /// turn instead keeps rolling and reaches its normal (forced-synthesis) completion.
    #[tokio::test(flavor = "current_thread")]
    async fn browser_done_does_not_terminate_a_non_browser_subturn() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        ls.tool_schemas = vec![json!({
            "type": "function",
            "function": {
                "name": "browser_done",
                "parameters": { "type": "object", "properties": {} }
            }
        })];
        let model = BrowserDoneModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = DoneBrowser;
        let mut config = cfg();
        config.browser_subturn = false;

        let outcome = run_turn(
            ls,
            config,
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        // The browser_done terminal did NOT fire: `memory_answer` is NOT the raw tool result
        // ("done") and the model was called a SECOND time (forced synthesis actually ran),
        // unlike the subturn case above where exactly one model call happens.
        assert_eq!(outcome.stop, crate::TurnStop::Completed);
        assert_ne!(outcome.memory_answer, "done");
        assert_eq!(outcome.memory_answer, "forced synthesis should not run");
        assert_eq!(
            model.calls.load(Ordering::SeqCst),
            2,
            "outside the browser sub-turn, browser_done must NOT short-circuit the loop — the turn \
             continues into a normal second round"
        );
    }

    #[derive(Default)]
    struct BlockedBrowserModel {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl ModelClient for BlockedBrowserModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let call_index = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if call.is_final_round {
                let content = "‹‹PAYMENT_APPROVAL››{\"snapshot\":{\"approval_id\":\"pay_unsafe\",\"merchant\":\"Stripe Elements Demo\",\"domain\":\"checkout.stripe.dev\",\"amount_minor\":12196,\"currency\":\"USD\",\"product_summary\":\"Demo checkout\",\"payment_method_label\":\"Test card 4242\",\"checkout_fingerprint\":\"demo\"}}‹‹/PAYMENT_APPROVAL››\nProcedi pure al pagamento.";
                on_delta(content);
                return Ok(ModelRoundOutput {
                    message: json!({ "role": "assistant", "content": content }),
                    provider,
                    finish_reason: Some("stop".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                });
            }

            Ok(ModelRoundOutput {
                message: json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": format!("browser_{call_index}"),
                        "type": "function",
                        "function": {
                            "name": "browser_navigate",
                            "arguments": format!("{{\"url\":\"https://example.com/{call_index}\"}}")
                        },
                    }],
                }),
                provider,
                finish_reason: Some("tool_calls".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    struct BlockedBrowser;

    impl BrowserExecutor for BlockedBrowser {
        async fn execute_browser(
            &mut self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            state: &mut LoopState,
        ) -> ToolOutcome {
            state.browser_used = true;
            browser_outcome(
                json!({ "status": "blocked" }).to_string(),
                crate::contract::ToolOutcomeHint::NoProgress,
            )
        }

        async fn close_session(&mut self, _browser_used: bool) {}
    }
    #[derive(Default)]
    struct StaleRefChurnModel {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl ModelClient for StaleRefChurnModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let call_index = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if call.is_final_round {
                let content =
                    "Non sono riuscito ad accedere alla pagina richiesta. Puoi riprovare.";
                on_delta(content);
                return Ok(ModelRoundOutput {
                    message: json!({ "role": "assistant", "content": content }),
                    provider,
                    finish_reason: Some("stop".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                });
            }
            // The model keeps targeting a ref that just went stale — exactly the SPA churn MINOR 8
            // guards against: the page keeps re-rendering the same control under a NEW [ref=eN].
            Ok(ModelRoundOutput {
                message: json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": format!("act_{call_index}"),
                        "type": "function",
                        "function": {
                            "name": "browser_act",
                            "arguments": "{\"kind\":\"click\",\"ref\":\"e1\"}"
                        },
                    }],
                }),
                provider,
                finish_reason: Some("tool_calls".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    /// Every `browser_act` comes back as the gateway's stale-ref auto-recovery text (MINOR 8): a
    /// real `Ok(...)` result (plain prose, not `{"status":...}`) that pre-fix would have looked
    /// like ordinary success to `classify_tool_result` and reset `browser_no_progress` on every
    /// call — letting the churn above loop forever instead of tripping the budget.
    struct StaleRefRecoveryBrowser;

    impl BrowserExecutor for StaleRefRecoveryBrowser {
        async fn execute_browser(
            &mut self,
            name: &str,
            _args: &str,
            _call_id: &str,
            state: &mut LoopState,
        ) -> ToolOutcome {
            assert_eq!(name, "browser_act");
            state.browser_used = true;
            // Hint = Success on purpose: this fixture proves the stale-ref MARKER trips
            // no-progress independently (the `||` in the branch), not the hint.
            browser_outcome(
                format!(
                    "{} I took a fresh snapshot. Do NOT retry e1; choose a NEW [ref=...] \
from this snapshot:\n[ref=e2] Same control, re-rendered",
                    crate::browser::STALE_REF_RECOVERY_MARKER
                ),
                crate::contract::ToolOutcomeHint::Success,
            )
        }

        async fn close_session(&mut self, _browser_used: bool) {}
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stale_ref_recovery_churn_still_trips_the_no_progress_budget() {
        // MINOR 8 regression: N consecutive stale-ref recoveries must NOT reset
        // `browser_no_progress` to 0 on every round (that would let a ref-churning SPA loop
        // act→stale→snapshot→act forever) — they must count as stalls like empty/error/blocked.
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = StaleRefRecoveryBrowser;
        let composio_writes = std::collections::BTreeSet::new();
        let catalog_index: Vec<(String, String, Value)> = Vec::new();
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 12;
        turn_cfg.browser_max_rounds = 12;
        turn_cfg.browser_nav_cap = 12;
        turn_cfg.browser_budget.max_no_progress = 2;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &StaleRefChurnModel::default(),
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &composio_writes,
            &catalog_index,
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let journal_events = journal.0.lock().unwrap();
        assert_eq!(
            journal_events
                .iter()
                .filter(|kind| kind.as_str() == "browser_budget_exceeded")
                .count(),
            1,
            "the budget must trip after max_no_progress consecutive stale-ref recoveries, not loop \
             until hard_round_ceiling"
        );
        drop(journal_events);
        assert_browser_budget_failed(&outcome);
    }

    /// Like `StaleRefChurnModel` but each round targets a DIFFERENT ref — a model legitimately
    /// advancing through a form, one field per round. Needed by the stall-window reset test:
    /// byte-identical rounds would trip the repeat-guard before the timing is provable.
    #[derive(Default)]
    struct VariedActModel {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl ModelClient for VariedActModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let call_index = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if call.is_final_round {
                let content = "Modulo completato.";
                on_delta(content);
                return Ok(ModelRoundOutput {
                    message: json!({ "role": "assistant", "content": content }),
                    provider,
                    finish_reason: Some("stop".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                });
            }
            Ok(ModelRoundOutput {
                message: json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": format!("act_{call_index}"),
                        "type": "function",
                        "function": {
                            "name": "browser_act",
                            "arguments": format!("{{\"kind\":\"click\",\"ref\":\"e{call_index}\"}}")
                        },
                    }],
                }),
                provider,
                finish_reason: Some("tool_calls".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    /// Progressing browser: every action takes real time (~20ms) and reports Success — the shape
    /// of a slow model legitimately advancing through a multi-field form. Used to prove the stall
    /// window RESETS on progress (the run outlives `max_stall_ms` cumulatively).
    struct SlowProgressBrowser;

    impl BrowserExecutor for SlowProgressBrowser {
        async fn execute_browser(
            &mut self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            state: &mut LoopState,
        ) -> ToolOutcome {
            state.browser_used = true;
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            browser_outcome(
                "Action performed. Updated snapshot:\n[ref=e2] Next field".to_string(),
                crate::contract::ToolOutcomeHint::Success,
            )
        }

        async fn close_session(&mut self, _browser_used: bool) {}
    }

    /// Stalled browser: every action takes longer than the stall window and reports NoProgress —
    /// the wall-clock stall window must stop the run even when the stagnation counters are far
    /// from tripping.
    struct SlowStallBrowser;

    impl BrowserExecutor for SlowStallBrowser {
        async fn execute_browser(
            &mut self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            state: &mut LoopState,
        ) -> ToolOutcome {
            state.browser_used = true;
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            browser_outcome(
                "Action performed. Updated snapshot:\n[ref=e1] Same field".to_string(),
                crate::contract::ToolOutcomeHint::NoProgress,
            )
        }

        async fn close_session(&mut self, _browser_used: bool) {}
    }

    #[derive(Default)]
    struct ReusedProviderToolCallIdModel {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl ModelClient for ReusedProviderToolCallIdModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let call_index = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if call_index < 2 {
                return Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": "ollama_call_0",
                            "type": "function",
                            "function": {
                                "name": "browser_act",
                                "arguments": format!("{{\"kind\":\"type\",\"ref\":\"e{call_index}\",\"text\":\"Milano\"}}")
                            },
                        }],
                    }),
                    provider,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                });
            }
            let content = "Browser task complete.";
            on_delta(content);
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": content }),
                provider,
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[derive(Default)]
    struct RecordingBrowserCallIds {
        ids: std::sync::Mutex<Vec<String>>,
    }

    impl BrowserExecutor for RecordingBrowserCallIds {
        async fn execute_browser(
            &mut self,
            _name: &str,
            _args: &str,
            call_id: &str,
            state: &mut LoopState,
        ) -> ToolOutcome {
            state.browser_used = true;
            self.ids.lock().unwrap().push(call_id.to_string());
            browser_outcome(
                "Action performed. Updated snapshot:\n[ref=e1] Next field".to_string(),
                crate::contract::ToolOutcomeHint::Success,
            )
        }

        async fn close_session(&mut self, _browser_used: bool) {}
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reused_provider_tool_call_ids_are_made_unique_across_rounds() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = RecordingBrowserCallIds::default();
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 8;
        turn_cfg.browser_max_rounds = 8;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &ReusedProviderToolCallIdModel::default(),
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(outcome.stop, crate::TurnStop::Completed);
        let ids = browser.ids.lock().unwrap().clone();
        assert_eq!(ids.len(), 2);
        assert_ne!(
            ids[0], ids[1],
            "provider-synthesized ids reused across rounds must not collapse browser effects"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stall_window_resets_on_progress_so_a_progressing_browse_outlives_it() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = SlowProgressBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 6;
        turn_cfg.browser_max_rounds = 6;
        turn_cfg.browser_nav_cap = 12;
        // Stall window (100ms) far below the run's TOTAL browse time (6 × 20ms + overhead): only
        // the per-progress reset lets this run finish without a budget stop.
        turn_cfg.browser_budget.max_stall_ms = 100;
        let started = Instant::now();

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &VariedActModel::default(),
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert!(
            started.elapsed() >= std::time::Duration::from_millis(100),
            "the run must have cumulatively outlived the stall window for the reset to be proven"
        );
        let journal_events = journal.0.lock().unwrap();
        assert_eq!(
            journal_events
                .iter()
                .filter(|kind| kind.as_str() == "browser_budget_exceeded")
                .count(),
            0,
            "a browse that makes progress every round must never be stopped by the stall window"
        );
        drop(journal_events);
        assert_eq!(outcome.stop, crate::TurnStop::Completed);
    }

    /// Makes real progress on every call and counts how many times it ran, so a test can prove the
    /// ROUND budget resets on browser progress (not just plan-frontier progress).
    struct CountingProgressBrowser(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl BrowserExecutor for CountingProgressBrowser {
        async fn execute_browser(
            &mut self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            state: &mut LoopState,
        ) -> ToolOutcome {
            state.browser_used = true;
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            browser_outcome(
                "Action performed. Updated snapshot:\n[ref=e2] Next field".to_string(),
                crate::contract::ToolOutcomeHint::Success,
            )
        }

        async fn close_session(&mut self, _browser_used: bool) {}
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_repeated_action_is_told_to_change_approach_before_the_turn_gives_up() {
        // The loop used to break out of the turn on the SECOND identical round, so a model stuck on
        // one form field lost the whole task and the user got a truncated answer with no explanation.
        // Being stuck on one step is a reason to tell the model to change approach — an autonomous run
        // may legitimately take a long time; what must not happen is spending it on the same failing
        // call. Assert the hint is emitted, and that the run keeps working past the old break point.
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut browser = CountingProgressBrowser(count.clone());
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 12;
        turn_cfg.browser_max_rounds = 12;
        turn_cfg.browser_nav_cap = 24;
        turn_cfg.browser_budget.max_no_progress = 50;
        turn_cfg.browser_budget.max_stall_ms = 600_000;

        let _ = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            // Emits the byte-identical browser_act every round → trips the repeat guard.
            &StaleRefChurnModel::default(),
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let events = sink.0.lock().unwrap();
        let text = events
            .iter()
            .filter_map(|event| match event {
                GenerateStreamEvent::Delta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("change approach"),
            "the model must be told to change approach before the turn is abandoned: {text}"
        );
        drop(events);
        assert!(
            count.load(std::sync::atomic::Ordering::SeqCst) > 2,
            "the turn must keep working past the old give-up point (2 identical rounds)"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn progressing_browse_runs_past_the_round_cap_because_progress_resets_it() {
        // The Trenitalia round-9 bug: `browse_round_budget` is passed as `browser_max_rounds`, and
        // the round cap at the top of the loop is `rounds_since_progress >= max_rounds`. But
        // `progress_anchor_round` reset ONLY on plan-frontier progress (a closed step) — never on
        // browser progress. A browse sub-turn has no plan, so the anchor stayed at 0 and a browse
        // advancing a form field every round was still cut off at `browser_max_rounds` while making
        // steady successful progress. A progressing browse must run PAST the round cap; only a real
        // stall (`stop_reason`: no_progress / stall window / absolute cap) may stop it.
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut browser = CountingProgressBrowser(count.clone());
        let mut turn_cfg = cfg();
        // Ceiling deliberately well ABOVE the round cap, so "capped at the cap" and "ran past it"
        // are distinguishable outcomes.
        turn_cfg.hard_round_ceiling = 12;
        turn_cfg.browser_max_rounds = 4;
        turn_cfg.browser_nav_cap = 24;
        // Neither stagnation counter nor stall window may interfere: only the round cap is on trial,
        // and it must NOT be what stops a browse that progresses every round.
        turn_cfg.browser_budget.max_no_progress = 50;
        turn_cfg.browser_budget.max_stall_ms = 600_000;
        let round_cap = turn_cfg.browser_max_rounds;

        let _ = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &VariedActModel::default(),
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let ran = count.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            ran > round_cap,
            "a browse that progresses every round must run PAST browser_max_rounds ({round_cap}), \
             not be capped at it — only {ran} browser actions ran"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stalled_browse_stops_within_the_stall_window() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = SlowStallBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 12;
        turn_cfg.browser_max_rounds = 12;
        turn_cfg.browser_nav_cap = 12;
        turn_cfg.browser_budget.max_stall_ms = 100;
        // Stagnation counters far from tripping: ONLY the stall window can stop this run.
        turn_cfg.browser_budget.max_no_progress = 50;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &StaleRefChurnModel::default(),
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let journal_events = journal.0.lock().unwrap();
        assert_eq!(
            journal_events
                .iter()
                .filter(|kind| kind.as_str() == "browser_budget_exceeded")
                .count(),
            1,
            "a browse with no progress for longer than the stall window must stop (reason: stall)"
        );
        drop(journal_events);
        assert_browser_budget_failed(&outcome);
    }

    /// Every `browser_act` comes back with PROSE that reads like success ("Action performed.
    /// Updated snapshot…") but a NoProgress HINT — the exact shape of a type that selected no
    /// autocomplete suggestion, or a timed-out action the sidecar reports as failed. The hint,
    /// not `classify_tool_result` on the prose, must drive the stall budget. Pre-fix (the browser
    /// branch discarded the hint and used `ToolEffects::default()`) this looped to the round
    /// ceiling; now `max_no_progress` trips.
    struct SuccessProseNoProgressBrowser;

    impl BrowserExecutor for SuccessProseNoProgressBrowser {
        async fn execute_browser(
            &mut self,
            name: &str,
            _args: &str,
            _call_id: &str,
            state: &mut LoopState,
        ) -> ToolOutcome {
            assert_eq!(name, "browser_act");
            state.browser_used = true;
            browser_outcome(
                "Action performed. Updated snapshot:\n[ref=e1] Station field".to_string(),
                crate::contract::ToolOutcomeHint::NoProgress,
            )
        }

        async fn close_session(&mut self, _browser_used: bool) {}
    }

    #[tokio::test(flavor = "current_thread")]
    async fn no_progress_hint_trips_budget_even_when_prose_looks_like_success() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = SuccessProseNoProgressBrowser;
        let composio_writes = std::collections::BTreeSet::new();
        let catalog_index: Vec<(String, String, Value)> = Vec::new();
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 12;
        turn_cfg.browser_max_rounds = 12;
        turn_cfg.browser_nav_cap = 12;
        turn_cfg.browser_budget.max_no_progress = 2;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &StaleRefChurnModel::default(),
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &composio_writes,
            &catalog_index,
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let journal_events = journal.0.lock().unwrap();
        assert_eq!(
            journal_events
                .iter()
                .filter(|kind| kind.as_str() == "browser_budget_exceeded")
                .count(),
            1,
            "a NoProgress hint must trip max_no_progress even though the result prose reads as success"
        );
        drop(journal_events);
        assert_browser_budget_failed(&outcome);
    }

    struct NoPlan;
    impl PlanProgress for NoPlan {
        async fn persist_plan(&self, _t: Option<&str>, _g: Option<&str>, _s: &[Value]) {}
        async fn record_step_outcome(&self, _t: Option<&str>, _s: &Value, _e: &[String]) {}
        async fn verify_step_complete(&self, _t: &str, _c: &str, _e: &str) -> (bool, String) {
            (false, String::new())
        }
        fn reconcile_on_delivery(&self, _p: &Value, _d: &str) -> Option<Vec<Value>> {
            None
        }
        fn plan_value_from_steps(&self, _g: Option<&str>, _s: &[Value]) -> Value {
            Value::Null
        }
    }
    #[derive(Default)]
    struct RecordingPlan(Mutex<Vec<Vec<Value>>>);
    impl PlanProgress for RecordingPlan {
        async fn persist_plan(&self, _t: Option<&str>, _g: Option<&str>, steps: &[Value]) {
            self.0.lock().unwrap().push(steps.to_vec());
        }
        async fn record_step_outcome(&self, _t: Option<&str>, _s: &Value, _e: &[String]) {}
        async fn verify_step_complete(&self, _t: &str, _c: &str, _e: &str) -> (bool, String) {
            (false, String::new())
        }
        fn reconcile_on_delivery(&self, _p: &Value, _d: &str) -> Option<Vec<Value>> {
            None
        }
        fn plan_value_from_steps(&self, goal: Option<&str>, steps: &[Value]) -> Value {
            json!({"goal": goal, "steps": steps})
        }
    }
    struct DoneJudge;
    impl TurnCompletionJudge for DoneJudge {
        async fn task_appears_incomplete(&self, _r: &str, _w: &str) -> bool {
            false
        }
    }
    struct NoCompact;
    impl ContextCompactor for NoCompact {
        async fn compact(&self, _m: &mut Vec<Value>, _s: &mut usize) -> bool {
            false
        }
    }
    #[derive(Default)]
    struct ReadRecordingCompactor(Mutex<Vec<TurnMemoryReadSet>>);
    impl ContextCompactor for ReadRecordingCompactor {
        async fn compact(&self, _m: &mut Vec<Value>, _s: &mut usize) -> bool {
            false
        }

        async fn compact_for_budget(
            &self,
            _messages: &mut Vec<Value>,
            _context_window: Option<usize>,
            memory_reads: &TurnMemoryReadSet,
        ) -> bool {
            self.0.lock().unwrap().push(memory_reads.clone());
            false
        }
    }
    struct OpenPolicy;
    impl TurnPolicy for OpenPolicy {
        fn route_blocked(&self, _t: &str) -> Option<String> {
            None
        }
        fn supports_vision(&self, _b: &str, _m: &str) -> bool {
            true
        }
    }
    #[derive(Default)]
    struct Collect(Mutex<Vec<GenerateStreamEvent>>);
    impl EventSink for Collect {
        async fn emit(&self, e: GenerateStreamEvent) {
            self.0.lock().unwrap().push(e);
        }
    }
    #[derive(Default)]
    struct CollectJournal(Mutex<Vec<String>>, Mutex<Vec<crate::LoopCheckpoint>>);
    impl crate::ExecutionJournal for CollectJournal {
        fn record(&self, event: crate::AgentExecutionEvent) {
            self.0
                .lock()
                .unwrap()
                .push(event.into_parts().0.to_string());
        }

        fn checkpoint(&self, checkpoint: crate::LoopCheckpoint) {
            self.1.lock().unwrap().push(checkpoint);
        }
    }

    struct ReportingCompactor {
        step_mutates: bool,
        budget_mutates: bool,
    }

    impl ContextCompactor for ReportingCompactor {
        async fn compact(&self, messages: &mut Vec<Value>, _start: &mut usize) -> bool {
            if self.step_mutates {
                messages.push(json!({"role": "assistant", "content": "step summary"}));
            }
            self.step_mutates
        }

        async fn compact_for_budget(
            &self,
            messages: &mut Vec<Value>,
            _context_window: Option<usize>,
            _memory_reads: &TurnMemoryReadSet,
        ) -> bool {
            if self.budget_mutates {
                messages.push(json!({"role": "assistant", "content": "budget summary"}));
            }
            self.budget_mutates
        }
    }

    #[derive(Default)]
    struct ContextEventJournal(Mutex<Vec<(String, Value)>>);

    impl crate::ExecutionJournal for ContextEventJournal {
        fn record(&self, event: crate::AgentExecutionEvent) {
            let (kind, _, payload) = event.into_parts();
            self.0.lock().unwrap().push((kind.to_string(), payload));
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn compaction_journal_records_only_actual_mutations_for_both_paths() {
        let mut state = LoopState::new();
        state.messages = vec![json!({"role": "user", "content": "request"})];
        state.pending_compaction = true;
        let journal = ContextEventJournal::default();

        apply_context_compaction_at_round_boundary(
            &mut state,
            &ReportingCompactor {
                step_mutates: false,
                budget_mutates: false,
            },
            &journal,
            3,
            Some(8_192),
        )
        .await;
        assert!(!state.pending_compaction);
        assert!(journal.0.lock().unwrap().is_empty());

        state.pending_compaction = true;
        apply_context_compaction_at_round_boundary(
            &mut state,
            &ReportingCompactor {
                step_mutates: true,
                budget_mutates: true,
            },
            &journal,
            4,
            Some(8_192),
        )
        .await;

        assert_eq!(
            *journal.0.lock().unwrap(),
            vec![
                (
                    "context_compacted".to_string(),
                    json!({"reason": "verified_step_boundary"}),
                ),
                (
                    "context_compacted".to_string(),
                    json!({"reason": "context_budget"}),
                ),
            ]
        );
    }

    fn cfg() -> TurnConfig {
        TurnConfig {
            hard_round_ceiling: 3,
            max_rounds: 2,
            browser_max_rounds: 8,
            browser_nav_cap: 6,
            browser_budget: crate::BrowserBudget {
                max_elapsed_ms: 300_000,
                max_stall_ms: 300_000,
                max_failed_navigations: 8,
                max_no_progress: 5,
            },
            context_window: None,
            reconcile_on_delivery: true,
            autoadvance_from_evidence: true,
            step_verification: true,
            verbose: false,
            forced_tool: None,
            // Default true: most existing browser_done tests in this module drive the browse
            // sub-turn shape and expect the terminal to fire. Tests that need the OFF behavior
            // (non-subturn) override this field explicitly.
            browser_subturn: true,
            resolved_hitl: None,
        }
    }

    fn usage_context() -> local_first_inference_usage::UsageContext {
        local_first_inference_usage::UsageContext::new(
            "test-turn",
            local_first_inference_usage::InferencePurpose::ChatResponse,
            "test-user",
        )
    }

    fn assert_browser_budget_failed(outcome: &crate::TurnOutcome) {
        assert_browser_failure_code(outcome, "browser_budget_exceeded");
    }

    fn assert_browser_failure_code(outcome: &crate::TurnOutcome, code: &str) {
        let crate::TurnStop::Failed { failure } = &outcome.stop else {
            panic!("browser incomplete work must fail the turn");
        };
        assert_eq!(failure.code, code);
    }

    #[test]
    fn browser_incomplete_exits_are_failure_classified() {
        for (reason, code) in [
            ("browser_budget_exceeded", "browser_budget_exceeded"),
            ("structured_no_progress", "browser_structured_no_progress"),
            (
                "round_budget_since_last_progress",
                "browser_round_budget_exceeded",
            ),
            ("browser_nav_cap_reached", "browser_navigation_cap_reached"),
        ] {
            assert!(browser_incomplete_loop_exit(Some(reason)));
            assert_eq!(browser_incomplete_failure_code(Some(reason)), code);
        }
        assert!(!browser_incomplete_loop_exit(Some(
            "model_stopped_naturally"
        )));
    }

    #[test]
    fn evidence_provenance_keeps_read_scope_and_drops_commands_and_secrets() {
        assert_eq!(
            evidence_argument_provenance(
                "mcp__project-files__read_text_file",
                r#"{"path":"/Users/fabio/Projects/Homun/app/README.md"}"#,
            )
            .as_deref(),
            Some("path=/Users/fabio/Projects/Homun/app/README.md")
        );
        assert!(
            evidence_argument_provenance("run_in_sandbox", r#"{"command":"cat /secret"}"#)
                .is_none()
        );
        assert!(
            evidence_argument_provenance(
                "connector_call",
                r#"{"api_key":"sk-secret","token":"private"}"#,
            )
            .is_none()
        );
    }

    #[test]
    fn plan_bookkeeping_never_counts_as_completion_evidence() {
        assert!(is_plan_bookkeeping_tool("update_plan"));
        assert!(is_plan_bookkeeping_tool("step_advance"));
        assert!(!is_plan_bookkeeping_tool("run_in_project"));
    }

    #[test]
    fn introspective_tools_are_exempt_from_plan_gate() {
        // Planning tools — must never trigger the gate.
        assert!(is_introspective_tool("update_plan"));
        assert!(is_introspective_tool("step_advance"));
        assert!(is_introspective_tool("recall_memory"));
        assert!(is_introspective_tool("find_capability"));
        assert!(is_introspective_tool("suggest_capabilities"));
        assert!(is_introspective_tool("use_skill"));
        // Work tools — the gate SHOULD fire for these.
        assert!(!is_introspective_tool("make_document"));
        assert!(!is_introspective_tool("run_in_project"));
        assert!(!is_introspective_tool("browser_navigate"));
        assert!(!is_introspective_tool("connector_send"));
        assert!(!is_introspective_tool("read_file"));
    }

    #[test]
    fn imperative_signal_count_combines_exclamations_and_verbs() {
        // No signals.
        assert_eq!(count_imperative_signals("hello world"), 0);
        // One exclamation.
        assert_eq!(count_imperative_signals("do it now!"), 1);
        // Two exclamations.
        assert_eq!(count_imperative_signals("do this! then that!"), 2);
        // One action verb (no exclamation).
        assert_eq!(count_imperative_signals("please create the file"), 1);
        // Two distinct action verbs.
        assert_eq!(count_imperative_signals("create and deploy the app"), 2);
        // Exclamation + verb = 2 signals (complex).
        assert_eq!(count_imperative_signals("create the report now!"), 2);
        // Repeated verb counts once (distinct verbs only).
        assert_eq!(count_imperative_signals("create and create again"), 1);
    }

    #[test]
    fn request_is_complex_heuristic() {
        // --- Simple requests: all conditions false → not complex ---
        let simple = "hi";
        assert!(!request_is_complex(simple, &[]));
        // Short message, single tool → not complex.
        let one_tool = vec![json!({"function": {"name": "make_document"}})];
        assert!(!request_is_complex(simple, &one_tool));

        // --- Complex by length ---
        let long_msg = "a".repeat(81);
        assert!(request_is_complex(&long_msg, &[]));
        // Exactly 80 is NOT complex (boundary).
        let exactly_80 = "a".repeat(80);
        assert!(!request_is_complex(&exactly_80, &[]));

        // --- Complex by imperative signals ---
        assert!(request_is_complex("create the report and deploy it", &[]));
        assert!(request_is_complex("do this! then that!", &[]));

        // --- Complex by multi-tool (2+ distinct tools) ---
        let two_tools = vec![
            json!({"function": {"name": "make_document"}}),
            json!({"function": {"name": "run_in_project"}}),
        ];
        assert!(request_is_complex("short", &two_tools));
        // Same tool twice is only 1 distinct → not complex by this condition.
        let same_tool_twice = vec![
            json!({"function": {"name": "make_document"}}),
            json!({"function": {"name": "make_document"}}),
        ];
        assert!(!request_is_complex("short", &same_tool_twice));
    }

    #[test]
    fn analytical_candidate_answer_becomes_labeled_bounded_evidence() {
        let content = format!(
            "## Contract table\n{}",
            (0..1200)
                .map(|index| format!("grounded-row-{index}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let evidence = candidate_answer_evidence(&content).expect("substantial answer evidence");

        assert!(evidence.contains("assistant_candidate_output"));
        assert!(evidence.contains("Contract table"));
        assert!(evidence.chars().count() < 3400);
        assert!(candidate_answer_evidence(&"Repeated grounded row. ".repeat(20)).is_some());
        assert!(candidate_answer_evidence("Short answer").is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tool_call_round_text_is_not_preserved_as_visible_history() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "use a tool" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let model = ToolCallTextLeakModel::default();

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &Vec::new(),
            "use a tool".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(outcome.stop, crate::TurnStop::Completed);
        let second_round_messages = model.second_round_messages.lock().unwrap();
        assert!(
            second_round_messages.iter().all(|message| message
                .get("content")
                .and_then(Value::as_str)
                .is_none_or(|content| !content.contains("Planning text"))),
            "tool-call planning text leaked into model-visible history: {second_round_messages:?}"
        );
        let assistant_tool_message = second_round_messages
            .iter()
            .find(|message| {
                message.get("role").and_then(Value::as_str) == Some("assistant")
                    && message
                        .get("tool_calls")
                        .and_then(Value::as_array)
                        .is_some()
            })
            .expect("assistant tool-call message should be preserved structurally");
        assert_eq!(
            assistant_tool_message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            ""
        );
    }

    // ⭐ The FIRST actual execution of `run_turn` (everything else is compile-time): drive a full turn
    // with mock seams and no network. Proves the extracted loop runs the happy path (model answers with
    // no tool calls) to completion — no panic, no hang (bounded by hard_round_ceiling), correct outcome.
    #[tokio::test(flavor = "current_thread")]
    async fn run_turn_happy_path_finishes_with_the_model_answer() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "hi" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let composio_writes = std::collections::BTreeSet::new();
        let catalog_index: Vec<(String, String, Value)> = Vec::new();

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &AnswerModel,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &composio_writes,
            &catalog_index,
            "hi".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(
            journal.0.lock().unwrap().as_slice(),
            ["prompt_snapshot", "model_response"]
        );

        // The model answered immediately → the turn commits that answer and ends.
        assert!(
            outcome.memory_answer.contains("Final answer"),
            "expected the model answer, got: {:?}",
            outcome.memory_answer
        );
        assert!(!outcome.memory_reads.has_linked_reads());
        assert_eq!(outcome.stop, crate::TurnStop::Completed);
        // A terminal Done event was emitted.
        assert!(
            sink.0
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, GenerateStreamEvent::Done { .. })),
            "expected a Done event"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn resumed_turn_rejects_same_wait_and_continues_to_terminal_answer() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "resume the resolved work" }),
            json!({ "role": "user", "content": "ALFA" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let model = ResolvedChoiceReplayThenAnswerModel::default();
        let mut turn_cfg = cfg();
        turn_cfg.resolved_hitl = Some(crate::hitl::ResolvedHitlGuard {
            envelope: crate::hitl::HitlEnvelope {
                kind: crate::hitl::HitlKind::Choice,
                hold_policy: crate::hitl::HoldPolicy::Free,
                payload: json!({
                    "question": "Which option should continue?",
                    "multi": false,
                    "options": ["ALFA", "BETA"]
                }),
                source_marker: "durable_resume".into(),
            },
            resolution: "ALFA".into(),
        });

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "ALFA".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(model.calls.load(Ordering::SeqCst), 2);
        assert_eq!(outcome.stop, crate::TurnStop::Completed);
        assert!(outcome.awaiting_user.is_none());
        assert_eq!(outcome.memory_answer, "SCELTA RIPRESA ALFA");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reasoning_only_completion_never_emits_done_or_claims_delivery() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "hi" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let model = ReasoningOnlyModel::default();
        let composio_writes = std::collections::BTreeSet::new();
        let catalog_index: Vec<(String, String, Value)> = Vec::new();

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &composio_writes,
            &catalog_index,
            "hi".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert!(
            matches!(outcome.stop, crate::TurnStop::Failed { .. }),
            "display-only text must not be delivered"
        );
        assert!(outcome.memory_answer.is_empty());
        assert_eq!(
            model.calls.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "the post-loop synthesis gets one chance to produce visible prose"
        );
        assert!(
            !sink
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, GenerateStreamEvent::Done { .. })),
            "display-only text must not emit Done"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn visible_answer_validation_preserves_structured_markers_in_done() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "hi" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let composio_writes = std::collections::BTreeSet::new();
        let catalog_index: Vec<(String, String, Value)> = Vec::new();

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &StructuredAnswerModel,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &composio_writes,
            &catalog_index,
            "hi".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(outcome.stop, crate::TurnStop::Completed);
        assert!(outcome.memory_answer.contains("‹‹PLAN››"));
        assert!(outcome.memory_answer.contains("‹‹ARTIFACT››"));
        let done_text = sink
            .0
            .lock()
            .unwrap()
            .iter()
            .find_map(|event| match event {
                GenerateStreamEvent::Done { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .expect("visible answer must emit Done");
        assert!(done_text.contains("‹‹PLAN››"));
        assert!(done_text.contains("‹‹ARTIFACT››"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forced_synthesis_keeps_prior_structured_output() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "make a document" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let model = ToolThenForcedSynthesisModel::default();
        let tool = StructuredOutputTool::default();
        let composio_writes = std::collections::BTreeSet::new();
        let catalog_index: Vec<(String, String, Value)> = Vec::new();

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &model,
            &tool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &composio_writes,
            &catalog_index,
            "make a document".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(outcome.stop, crate::TurnStop::Completed,);
        let expected = "‹‹PLAN››- [x] Deliver result‹‹/PLAN››\n‹‹ARTIFACT››report.md‹‹/ARTIFACT››\n‹‹CHOICES››choose a format‹‹/CHOICES››\n‹‹REASONING››synthesis reasoning‹‹/REASONING››\nForced synthesis answer.";
        assert_eq!(outcome.memory_answer, expected);
        assert_eq!(outcome.memory_answer.matches("‹‹PLAN››").count(), 1,);
        assert_eq!(outcome.memory_answer.matches("‹‹ARTIFACT››").count(), 1,);
        assert_eq!(outcome.memory_answer.matches("‹‹CHOICES››").count(), 1,);
        let done_text = sink
            .0
            .lock()
            .unwrap()
            .iter()
            .find_map(|event| match event {
                GenerateStreamEvent::Done { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .expect("forced synthesis must emit Done");
        assert_eq!(done_text, expected);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forced_synthesis_materializes_prose_choice_as_awaiting_user() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "book a train" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let model = ToolThenForcedSynthesisProseChoiceModel::default();
        let tool = StructuredOutputTool::default();
        let composio_writes = std::collections::BTreeSet::new();
        let catalog_index: Vec<(String, String, Value)> = Vec::new();

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &model,
            &tool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &composio_writes,
            &catalog_index,
            "book a train".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let envelope = outcome.awaiting_user.expect("forced synthesis prose wait");
        assert_eq!(envelope.kind, HitlKind::Choice);
        assert_eq!(envelope.hold_policy, crate::hitl::HoldPolicy::Free);
        assert_eq!(outcome.stop, crate::TurnStop::SuspendedUser);
        assert!(outcome.memory_answer.contains("‹‹CHOICES››"));
        let done_text = sink
            .0
            .lock()
            .unwrap()
            .iter()
            .find_map(|event| match event {
                GenerateStreamEvent::Done { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .expect("forced synthesis HITL must emit Done");
        assert!(done_text.contains("‹‹CHOICES››"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn browser_circuit_breaker_delivers_fallback_without_forced_synthesis() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = BlockedBrowser;
        let composio_writes = std::collections::BTreeSet::new();
        let catalog_index: Vec<(String, String, Value)> = Vec::new();
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 12;
        turn_cfg.browser_max_rounds = 12;
        turn_cfg.browser_nav_cap = 12;
        turn_cfg.browser_budget.max_no_progress = 2;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &BlockedBrowserModel::default(),
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &composio_writes,
            &catalog_index,
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let journal_events = journal.0.lock().unwrap();
        assert_eq!(
            journal_events
                .iter()
                .filter(|kind| kind.as_str() == "browser_budget_exceeded")
                .count(),
            1
        );
        assert_eq!(
            journal_events
                .iter()
                .filter(|kind| kind.as_str() == "forced_synthesis")
                .count(),
            0
        );
        drop(journal_events);
        let done_events = sink
            .0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                GenerateStreamEvent::Done { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(done_events.len(), 1);
        assert!(!done_events[0].contains("PAYMENT_APPROVAL"));
        assert!(done_events[0].contains("browser ha esaurito il budget"));
        assert_browser_budget_failed(&outcome);
        assert!(!outcome.memory_answer.contains("PAYMENT_APPROVAL"));
        assert!(outcome.memory_answer.contains("Non sono riuscito"));
    }

    #[derive(Default)]
    struct RecallThenAnswerModel {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl ModelClient for RecallThenAnswerModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": "recall_1",
                            "type": "function",
                            "function": { "name": "recall_memory", "arguments": "{}" },
                        }],
                    }),
                    provider,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                })
            } else {
                on_delta("Answer from linked memory.");
                Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "Answer from linked memory."
                    }),
                    provider,
                    finish_reason: Some("stop".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                })
            }
        }
    }

    struct LinkedRecallTool;

    impl CapabilityExecutor for LinkedRecallTool {
        async fn execute_tool(
            &self,
            name: &str,
            _args: &str,
            _call_id: &str,
            _state: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            assert_eq!(name, "recall_memory");
            Ok(ToolOutcome {
                result: "linked fact".to_string(),
                effects: ToolEffects {
                    memory_reads: TurnMemoryReadSet {
                        linked: vec![LinkedMemoryRead {
                            source_workspace_id: "source-a".to_string(),
                            grant_id: "grant-a".to_string(),
                            policy_version: 3,
                            memory_ref: "memory:owner:source-a:fact-a".to_string(),
                            source_revision: "sha256:rev-a".to_string(),
                        }],
                        blocked_unknown: false,
                    },
                    ..ToolEffects::default()
                },
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn recall_tool_reads_reach_turn_outcome() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "recall" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let composio_writes = std::collections::BTreeSet::new();
        let catalog_index: Vec<(String, String, Value)> = Vec::new();
        let compactor = ReadRecordingCompactor::default();

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &RecallThenAnswerModel::default(),
            &LinkedRecallTool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &compactor,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &composio_writes,
            &catalog_index,
            "recall".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(outcome.memory_reads.linked.len(), 1);
        assert_eq!(outcome.memory_reads.linked[0].grant_id, "grant-a");
        assert!(
            compactor
                .0
                .lock()
                .unwrap()
                .iter()
                .any(TurnMemoryReadSet::has_linked_reads)
        );
    }

    // Round-0-only mock: returns a `make_document` tool call on the FIRST `generate` invocation,
    // then plain text (no tool_calls) on every subsequent one. Records the `forced_tool` it was
    // called with on each round so the test can assert forcing was applied ONCE, not every round.
    #[derive(Default)]
    struct ToolThenAnswerModel {
        calls: std::sync::atomic::AtomicUsize,
        forced_tool_seen: Mutex<Vec<Option<String>>>,
    }
    impl ModelClient for ToolThenAnswerModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            self.forced_tool_seen
                .lock()
                .unwrap()
                .push(call.forced_tool.map(str::to_string));
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n == 0 {
                // Round 0: the provider-forced call — MUST return a tool call (that's the
                // contract of a forced tool_choice).
                Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": { "name": "make_document", "arguments": "{}" },
                        }],
                    }),
                    provider,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                })
            } else {
                // Round ≥1: the delivery already happened — a real model, back on "auto",
                // summarizes and stops. If forcing were still active here (the C1 bug), this
                // branch would never be reached: the provider would be compelled to emit
                // another tool call instead of text.
                on_delta("Document delivered.");
                Ok(ModelRoundOutput {
                    message: json!({ "role": "assistant", "content": "Document delivered." }),
                    provider,
                    finish_reason: Some("stop".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                })
            }
        }
    }

    // Counts how many times each tool name was dispatched, so the test can assert the forced
    // tool ran exactly once (not once per round, which was the C1 bug: force-looping the same
    // tool_choice every round meant a successful delivery was immediately followed by a SECOND,
    // now-unbound/generic call to the same tool).
    #[derive(Default)]
    struct CountingTool {
        make_document_calls: std::sync::atomic::AtomicUsize,
    }
    impl CapabilityExecutor for CountingTool {
        async fn execute_tool(
            &self,
            name: &str,
            _a: &str,
            _c: &str,
            _s: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            if name == "make_document" {
                self.make_document_calls
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            Ok(ToolOutcome {
                result: format!("ran {name}"),
                effects: ToolEffects::default(),
            })
        }
    }

    #[derive(Default)]
    struct WorkflowThenBlockedFallbackModel {
        calls: std::sync::atomic::AtomicUsize,
    }
    impl ModelClient for WorkflowThenBlockedFallbackModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            let message = match index {
                0 => json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "workflow_call",
                        "type": "function",
                        "function": { "name": "make_document", "arguments": "{}" },
                    }],
                }),
                1 => json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "blocked_fallback",
                        "type": "function",
                        "function": { "name": "update_plan", "arguments": "{}" },
                    }],
                }),
                _ => {
                    on_delta("Document delivered from the first workflow result.");
                    json!({
                        "role": "assistant",
                        "content": "Document delivered from the first workflow result."
                    })
                }
            };
            Ok(ModelRoundOutput {
                message,
                provider,
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[derive(Default)]
    struct TerminalWorkflowPolicy {
        workflow_calls: std::sync::atomic::AtomicUsize,
    }
    impl TurnPolicy for TerminalWorkflowPolicy {
        fn route_blocked(&self, tool: &str) -> Option<String> {
            if tool == "make_document"
                && self
                    .workflow_calls
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                    == 0
            {
                return None;
            }
            Some(format!("workflow route blocked {tool}"))
        }

        fn route_block_ends_turn(&self) -> bool {
            self.workflow_calls
                .load(std::sync::atomic::Ordering::SeqCst)
                > 0
        }

        fn supports_vision(&self, _base_url: &str, _model: &str) -> bool {
            true
        }
    }

    // ⭐ Final-review fix C1 (CRITICAL): forcing `tool_choice` must be ONE-SHOT within the turn.
    // Before the fix, `cfg.forced_tool` was passed on EVERY round's model call — since a forced
    // tool_choice contractually MUST come back with a tool call, the loop could never terminate
    // after a successful delivery: round 1 would force `make_document` AGAIN (a duplicate render,
    // by then unbound/generic since the routing binding had already cleared). This test drives a
    // turn with `forced_tool = Some("make_document")` and a mock model that returns the tool call
    // on round 0 and plain text on round 1+, and asserts the tool executed exactly ONCE and the
    // turn ends with the model's own text summary — not a second forced tool call.
    #[tokio::test(flavor = "current_thread")]
    async fn forced_tool_applies_only_to_round_zero_then_terminates_on_model_text() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "make me the quarterly report" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let composio_writes = std::collections::BTreeSet::new();
        let catalog_index: Vec<(String, String, Value)> = Vec::new();

        let model = ToolThenAnswerModel::default();
        let tool = CountingTool::default();

        let mut turn_cfg = cfg();
        turn_cfg.forced_tool = Some("make_document".to_string());
        // Give the loop enough round budget to reach round 1 (the post-delivery round) so the
        // fix's "later rounds fall back to auto" behavior is actually exercised.
        turn_cfg.hard_round_ceiling = 4;
        turn_cfg.max_rounds = 4;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &tool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &composio_writes,
            &catalog_index,
            "make me the quarterly report".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        // The tool ran exactly once — no duplicate render on round 1.
        assert_eq!(
            tool.make_document_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1,
            "make_document must execute exactly once, not once per round"
        );
        // Forcing was applied on round 0 only; round 1 (and any later round) saw `None` (auto).
        let seen = model.forced_tool_seen.lock().unwrap().clone();
        assert_eq!(
            seen.first().and_then(|f| f.as_deref()),
            Some("make_document"),
            "round 0 must be forced: {seen:?}"
        );
        assert!(
            seen.iter().skip(1).all(|f| f.is_none()),
            "every round after the first must be auto (None), got: {seen:?}"
        );
        // The turn ended with the model's own text summary, not a second forced tool call.
        assert!(
            outcome.memory_answer.contains("Document delivered"),
            "expected the model's post-delivery text summary, got: {:?}",
            outcome.memory_answer
        );
        // Exactly one Done event — the loop terminated cleanly instead of force-looping.
        let done_count = sink
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, GenerateStreamEvent::Done { .. }))
            .count();
        assert_eq!(
            done_count, 1,
            "expected exactly one Done event (clean termination)"
        );
    }

    // ------------------------------------------------------------------
    // Plan-before-act gate integration tests
    // ------------------------------------------------------------------

    /// Round 0: emit a `make_document` tool call. Round 1+: answer "Done!".
    #[derive(Default)]
    struct WorkThenAnswerModel {
        calls: std::sync::atomic::AtomicUsize,
    }
    impl ModelClient for WorkThenAnswerModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if n == 0 {
                Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": "call_0",
                            "type": "function",
                            "function": { "name": "make_document", "arguments": "{}" },
                        }],
                    }),
                    provider,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                })
            } else {
                on_delta("Done!");
                Ok(ModelRoundOutput {
                    message: json!({ "role": "assistant", "content": "Done!" }),
                    provider,
                    finish_reason: Some("stop".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                })
            }
        }
    }

    /// Round 0 and 1: `make_document` without any model-authored plan. The loop
    /// should bootstrap exactly one canonical plan before the first work tool.
    #[derive(Default)]
    struct ComplexIgnoreGateModel {
        calls: std::sync::atomic::AtomicUsize,
        round1_messages: Mutex<Vec<Value>>,
    }
    impl ModelClient for ComplexIgnoreGateModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if n <= 1 {
                if n == 1 {
                    *self.round1_messages.lock().unwrap() = call.messages.to_vec();
                }
                Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": format!("work_{n}"),
                            "type": "function",
                            "function": { "name": "make_document", "arguments": "{}" },
                        }],
                    }),
                    provider,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                })
            } else {
                on_delta("Done!");
                Ok(ModelRoundOutput {
                    message: json!({ "role": "assistant", "content": "Done!" }),
                    provider,
                    finish_reason: Some("stop".to_string()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                })
            }
        }
    }

    const COMPLEX_USER_MSG: &str = "Please create a detailed quarterly financial report with charts, analysis, and \
        recommendations for the board. Include revenue breakdowns, expense categories, and \
        year-over-year comparisons.";

    #[tokio::test(flavor = "current_thread")]
    async fn plan_gate_simple_request_passes_through() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "hi" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let model = WorkThenAnswerModel::default();
        let tool = CountingTool::default();
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 4;
        turn_cfg.max_rounds = 4;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &tool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "hi".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        // The tool executed (gate did NOT fire for a simple request).
        assert_eq!(
            tool.make_document_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1,
            "simple request must pass through the gate"
        );
        assert!(
            outcome.memory_answer.contains("Done!"),
            "expected the model's answer, got: {:?}",
            outcome.memory_answer
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn plan_gate_complex_request_without_plan_defers_tool_for_approval() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": COMPLEX_USER_MSG }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let model = WorkThenAnswerModel::default();
        let tool = CountingTool::default();
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 4;
        turn_cfg.max_rounds = 4;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &tool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            COMPLEX_USER_MSG.to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        // The gate defers the work tool and asks the model to propose a plan for approval
        // instead of executing unilaterally. The model (which answers "Done!" on the next
        // round) never re-issues the tool, so it is never executed.
        assert_eq!(
            tool.make_document_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            0,
            "the work tool must be deferred, not executed, before plan approval"
        );
        let events = sink.0.lock().unwrap();
        assert!(
            events.iter().any(|event| matches!(
                event,
                GenerateStreamEvent::Delta { text } if text.contains("Propongo un piano")
            )),
            "the loop must surface a plan-approval nudge to the UI"
        );
        drop(events);
        assert!(
            outcome.memory_answer.contains("Done!"),
            "expected the model's final answer, got: {:?}",
            outcome.memory_answer
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn plan_gate_complex_request_with_existing_plan_passes_through() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": COMPLEX_USER_MSG }),
        ];
        ls.step_messages_start = ls.messages.len();
        // Pre-seed the plan so the gate sees a non-empty plan and does NOT fire.
        ls.plan = json!({
            "steps": [
                {"id": "s1", "title": "Create report", "status": "done", "detail": ""}
            ]
        });
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let model = WorkThenAnswerModel::default();
        let tool = CountingTool::default();
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 4;
        turn_cfg.max_rounds = 4;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &tool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            COMPLEX_USER_MSG.to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        // The tool executed immediately — the gate did NOT fire because a plan already existed.
        assert_eq!(
            tool.make_document_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1,
            "complex request with an existing plan must pass through the gate"
        );
        assert!(
            outcome.memory_answer.contains("Done!"),
            "expected the model's answer, got: {:?}",
            outcome.memory_answer
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn plan_gate_ignoring_model_defers_once_then_executes() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": COMPLEX_USER_MSG }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let model = ComplexIgnoreGateModel::default();
        let tool = CountingTool::default();
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 5;
        turn_cfg.max_rounds = 5;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &tool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            COMPLEX_USER_MSG.to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        // The model ignores the approval nudge and re-issues the work tool on the next
        // round. The gate fires only once (the first time), defers that call, and lets the
        // second attempt through — so exactly one tool call executes.
        assert_eq!(
            tool.make_document_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the first work call is deferred, the second executes"
        );
        let msgs = model.round1_messages.lock().unwrap();
        let has_approval_directive = msgs.iter().any(|m| {
            m.get("role").and_then(|r| r.as_str()) == Some("user")
                && m.get("content")
                    .and_then(|c| c.as_str())
                    .is_some_and(|c| c.contains("propose a plan for the user to approve"))
        });
        assert!(
            has_approval_directive,
            "the deferred round must carry the plan-approval directive to the model"
        );
        assert!(
            outcome.memory_answer.contains("Done!"),
            "expected the model's final answer, got: {:?}",
            outcome.memory_answer
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn blocked_fallback_after_workflow_forces_one_final_synthesis() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "make the document" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let model = WorkflowThenBlockedFallbackModel::default();
        let tool = CountingTool::default();
        let policy = TerminalWorkflowPolicy::default();
        let mut turn_cfg = cfg();
        turn_cfg.forced_tool = Some("make_document".to_string());
        turn_cfg.hard_round_ceiling = 8;
        turn_cfg.max_rounds = 8;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &tool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &policy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "make the document".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(
            tool.make_document_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert_eq!(
            model.calls.load(std::sync::atomic::Ordering::SeqCst),
            3,
            "one workflow call, one blocked fallback, one final synthesis"
        );
        assert!(
            outcome
                .memory_answer
                .contains("Document delivered from the first workflow result")
        );
        assert_eq!(outcome.stop, crate::TurnStop::Completed);
    }

    #[derive(Default)]
    struct SteeringFinalizeModel {
        calls: AtomicUsize,
        control_ready: AtomicBool,
        applied: AtomicBool,
        completed: AtomicBool,
    }

    impl ModelClient for SteeringFinalizeModel {
        fn current_turn_control(&self) -> Option<crate::TurnControlDecision> {
            (self.control_ready.load(Ordering::SeqCst) && !self.applied.load(Ordering::SeqCst))
                .then(|| crate::TurnControlDecision {
                    steering_id: 7,
                    disposition: crate::TurnControlDisposition::FinalizeWithCurrentEvidence,
                    instruction: "Answer now from the evidence already collected".to_string(),
                })
        }

        fn acknowledge_turn_control_applied(&self, steering_id: i64) {
            assert_eq!(steering_id, 7);
            self.applied.store(true, Ordering::SeqCst);
        }

        async fn wait_for_turn_control(&self) -> crate::TurnControlDecision {
            std::future::poll_fn(|context| {
                if let Some(control) = self.current_turn_control() {
                    std::task::Poll::Ready(control)
                } else {
                    context.waker().wake_by_ref();
                    std::task::Poll::Pending
                }
            })
            .await
        }

        fn acknowledge_turn_control_completed(&self, steering_id: i64) {
            assert_eq!(steering_id, 7);
            self.completed.store(true, Ordering::SeqCst);
        }

        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let message = if index == 0 {
                json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "tool_wait",
                        "type": "function",
                        "function": { "name": "wait_forever", "arguments": "{}" }
                    }]
                })
            } else {
                assert!(call.tools.is_empty());
                assert!(call.messages.iter().any(|message| {
                    message["content"]
                        .as_str()
                        .is_some_and(|content| content.contains("evidence already collected"))
                }));
                json!({"role": "assistant", "content": "Final answer from current evidence."})
            };
            Ok(ModelRoundOutput {
                message,
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some(if index == 0 { "tool_calls" } else { "stop" }.to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    struct PendingSteeringTool<'a>(&'a SteeringFinalizeModel);

    impl CapabilityExecutor for PendingSteeringTool<'_> {
        async fn execute_tool(
            &self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            _state: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            self.0.control_ready.store(true, Ordering::SeqCst);
            std::future::pending().await
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn steering_control_interrupts_active_tool_and_forces_one_terminal_synthesis() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "investigate" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = SteeringFinalizeModel::default();
        let tool = PendingSteeringTool(&model);
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &model,
            &tool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "investigate".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(outcome.stop, crate::TurnStop::Completed);
        assert_eq!(model.calls.load(Ordering::SeqCst), 2);
        assert!(model.applied.load(Ordering::SeqCst));
        assert!(model.completed.load(Ordering::SeqCst));
        assert_eq!(
            sink.0
                .lock()
                .unwrap()
                .iter()
                .filter(|event| matches!(event, GenerateStreamEvent::Done { .. }))
                .count(),
            1
        );
    }

    #[derive(Default)]
    struct ExhaustedTurnSteeringModel {
        calls: AtomicUsize,
        control_ready: AtomicBool,
        applied: AtomicBool,
        completed: AtomicBool,
    }

    impl ModelClient for ExhaustedTurnSteeringModel {
        fn finalization_fence(&self) -> crate::FinalizationFence {
            if self.control_ready.load(Ordering::SeqCst) && !self.applied.load(Ordering::SeqCst) {
                crate::FinalizationFence::PendingInput
            } else {
                crate::FinalizationFence::Ready
            }
        }

        fn current_turn_control(&self) -> Option<crate::TurnControlDecision> {
            (self.control_ready.load(Ordering::SeqCst) && !self.applied.load(Ordering::SeqCst))
                .then(|| crate::TurnControlDecision {
                    steering_id: 9,
                    disposition: crate::TurnControlDisposition::FinalizeWithCurrentEvidence,
                    instruction: "Stop browsing and answer from current evidence".to_string(),
                })
        }

        async fn wait_for_turn_control(&self) -> crate::TurnControlDecision {
            loop {
                if let Some(control) = self.current_turn_control() {
                    return control;
                }
                tokio::task::yield_now().await;
            }
        }

        fn acknowledge_turn_control_applied(&self, steering_id: i64) {
            assert_eq!(steering_id, 9);
            self.applied.store(true, Ordering::SeqCst);
        }

        fn acknowledge_turn_control_completed(&self, steering_id: i64) {
            assert_eq!(steering_id, 9);
            self.completed.store(true, Ordering::SeqCst);
        }

        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let message = if index == 0 {
                json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "browse_once",
                        "type": "function",
                        "function": { "name": "browse", "arguments": "{}" }
                    }]
                })
            } else {
                assert!(call.is_final_round);
                assert!(self.applied.load(Ordering::SeqCst));
                assert!(call.messages.iter().any(|message| {
                    message["content"]
                        .as_str()
                        .is_some_and(|content| content.contains("Stop browsing"))
                }));
                json!({"role": "assistant", "content": "Answer after the steering instruction."})
            };
            Ok(ModelRoundOutput {
                message,
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some(if index == 0 { "tool_calls" } else { "stop" }.to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    struct ExhaustingBrowseTool<'a>(&'a ExhaustedTurnSteeringModel);

    impl CapabilityExecutor for ExhaustingBrowseTool<'_> {
        async fn execute_tool(
            &self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            _state: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            self.0.control_ready.store(true, Ordering::SeqCst);
            Ok(ToolOutcome {
                result: "found: false".to_string(),
                effects: ToolEffects::default(),
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn post_loop_synthesis_cannot_overtake_interpreted_steering() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "research" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = ExhaustedTurnSteeringModel::default();
        let tool = ExhaustingBrowseTool(&model);
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 1;
        turn_cfg.max_rounds = 1;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &tool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "research".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(outcome.stop, crate::TurnStop::Completed);
        assert!(model.applied.load(Ordering::SeqCst));
        assert!(model.completed.load(Ordering::SeqCst));
        assert_eq!(model.calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            sink.0
                .lock()
                .unwrap()
                .iter()
                .filter(|event| matches!(event, GenerateStreamEvent::Done { .. }))
                .count(),
            1
        );
    }

    #[derive(Default)]
    struct BrowserDeadlineModel {
        calls: AtomicUsize,
    }

    impl ModelClient for BrowserDeadlineModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let message = if index == 0 {
                json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "slow_browse",
                        "type": "function",
                        "function": { "name": "browse", "arguments": "{\"goal\":\"test\"}" }
                    }]
                })
            } else {
                assert!(call.is_final_round);
                json!({"role": "assistant", "content": "Final answer from the available evidence."})
            };
            Ok(ModelRoundOutput {
                message,
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some(if index == 0 { "tool_calls" } else { "stop" }.to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[derive(Default)]
    struct BrowserEmptyFinalModel {
        calls: AtomicUsize,
    }

    impl ModelClient for BrowserEmptyFinalModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let message = if index == 0 {
                json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "empty_browse",
                        "type": "function",
                        "function": { "name": "browse", "arguments": "{\"goal\":\"test\"}" }
                    }]
                })
            } else {
                assert!(call.is_final_round);
                json!({"role": "assistant", "content": ""})
            };
            Ok(ModelRoundOutput {
                message,
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some(if index == 0 { "tool_calls" } else { "stop" }.to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    struct SlowDelegatedBrowse;

    impl CapabilityExecutor for SlowDelegatedBrowse {
        async fn execute_tool(
            &self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            _state: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
            Ok(ToolOutcome {
                result: "found: true".to_string(),
                effects: ToolEffects {
                    browser_activity_observed: true,
                    outcome_hint: Some(crate::ToolOutcomeHint::Success),
                    ..ToolEffects::default()
                },
            })
        }
    }

    struct SlowNoProgressDelegatedBrowse;

    impl CapabilityExecutor for SlowNoProgressDelegatedBrowse {
        async fn execute_tool(
            &self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            _state: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
            Ok(ToolOutcome {
                result: "found: false".to_string(),
                effects: ToolEffects {
                    browser_activity_observed: true,
                    outcome_hint: Some(crate::ToolOutcomeHint::NoProgress),
                    ..ToolEffects::default()
                },
            })
        }
    }

    struct GroundedPartialDelegatedBrowse;

    impl CapabilityExecutor for GroundedPartialDelegatedBrowse {
        async fn execute_tool(
            &self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            _state: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
            let result = crate::BrowseResult {
                found: true,
                answer: "FR 9512 Milano Centrale -> Roma Termini 09:05 12:10 EUR 49.90\nItalo 9920 Milano Centrale -> Roma Termini 09:40 12:55 EUR 52.90".to_string(),
                sources: vec!["https://www.trenitalia.com/search".to_string()],
                confidence: crate::browse::Confidence::Low,
                note: None,
                status: crate::browse::BrowserDoneStatus::Partial,
                items: vec![json!({
                    "train": "FR 9512",
                    "departure": "09:05",
                    "arrival": "12:10",
                    "price": "EUR 49.90"
                })],
                fields_missing: vec!["browser_done".to_string()],
                evidence: vec!["grounded result snapshot captured before timeout".to_string()],
            };
            Ok(ToolOutcome {
                result: crate::browse::browse_result_for_manager(&result),
                effects: ToolEffects {
                    browser_activity_observed: true,
                    outcome_hint: Some(crate::ToolOutcomeHint::NoProgress),
                    ..ToolEffects::default()
                },
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn delegated_browse_is_interrupted_at_the_turns_remaining_browser_deadline() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = BrowserDeadlineModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.browser_budget.max_elapsed_ms = 20;
        let started = Instant::now();

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &SlowDelegatedBrowse,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert!(started.elapsed() < std::time::Duration::from_millis(100));
        assert_browser_budget_failed(&outcome);
        assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    }

    /// Round 0 delegates to `browse`; the next round answers from the result — no `is_final_round`
    /// expectation (the browse COMPLETES normally here, so round 1 is an ordinary round, unlike the
    /// budget-exhausted deadline test where the second call is a forced synthesis).
    #[derive(Default)]
    struct BrowseThenAnswerModel {
        calls: AtomicUsize,
    }

    impl ModelClient for BrowseThenAnswerModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let (message, finish) = if index == 0 {
                (
                    json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": "browse_0",
                            "type": "function",
                            "function": { "name": "browse", "arguments": "{\"goal\":\"test\"}" }
                        }]
                    }),
                    "tool_calls",
                )
            } else {
                (
                    json!({"role": "assistant", "content": "Answer using the browse result."}),
                    "stop",
                )
            };
            Ok(ModelRoundOutput {
                message,
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some(finish.to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[derive(Default)]
    struct NoProgressBrowseThenChoiceModel {
        calls: AtomicUsize,
    }

    impl ModelClient for NoProgressBrowseThenChoiceModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let message = if index == 0 {
                json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "browse_0",
                        "type": "function",
                        "function": { "name": "browse", "arguments": "{\"goal\":\"test\"}" }
                    }]
                })
            } else {
                json!({
                    "role": "assistant",
                    "content": r#"‹‹CHOICES››{"question":"Vuoi che riprovi?","options":["Si","No"]}‹‹/CHOICES››"#
                })
            };
            Ok(ModelRoundOutput {
                message,
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some(if index == 0 { "tool_calls" } else { "stop" }.to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_completed_delegated_browse_does_not_trip_the_manager_stall_window_on_return() {
        // C2 regression: a healthy `browse` that legitimately runs longer than the MANAGER turn's
        // stall window must NOT stall the manager the moment it returns — the model has to get its
        // next round to use the browse result. `browser_activity_observed` resets the manager stall
        // clock (the granular-branch reset never fires in the manager turn). Without the fix the
        // model would be cut off after the browse (1 call, budget_exceeded).
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = BrowseThenAnswerModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.browser_budget.max_elapsed_ms = 300_000; // browse completes (its 120ms << cap)
        turn_cfg.browser_budget.max_stall_ms = 80; // < the browse's 120ms → would stall on return

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &SlowDelegatedBrowse,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let stalls = journal
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|kind| kind.as_str() == "browser_budget_exceeded")
            .count();
        assert_eq!(
            stalls, 0,
            "a completed browse must not trip the manager stall window on return"
        );
        assert_eq!(
            model.calls.load(Ordering::SeqCst),
            2,
            "the model must get its post-browse round to use the result"
        );
        assert_eq!(outcome.stop, crate::TurnStop::Completed);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn no_progress_delegated_browse_terminalizes_without_user_wait() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = NoProgressBrowseThenChoiceModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.browser_budget.max_elapsed_ms = 300_000;
        turn_cfg.browser_budget.max_stall_ms = 300_000;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &SlowNoProgressDelegatedBrowse,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let crate::TurnStop::Failed { failure } = &outcome.stop else {
            panic!(
                "no-progress browse must fail canonically, got {:?}",
                outcome.stop
            );
        };
        assert_eq!(failure.code, "browser_structured_no_progress");
        assert_eq!(
            failure.class,
            local_first_execution_protocol::FailureClass::Permanent
        );
        assert!(outcome.awaiting_user.is_none());
        assert!(
            !outcome.memory_answer.contains("‹‹CHOICES››"),
            "{}",
            outcome.memory_answer
        );
        assert!(
            outcome.memory_answer.contains("Non sono riuscito"),
            "{}",
            outcome.memory_answer
        );
        assert_eq!(
            model.calls.load(Ordering::SeqCst),
            1,
            "browser no-progress must not invoke forced synthesis"
        );
        let forced_synthesis = journal
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|kind| kind.as_str() == "forced_synthesis")
            .count();
        assert_eq!(
            forced_synthesis, 0,
            "browser no-progress must terminalize without post-loop synthesis"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn no_progress_delegated_browse_terminalizes_before_manager_stall_window() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = BrowseThenAnswerModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.browser_budget.max_elapsed_ms = 300_000;
        turn_cfg.browser_budget.max_stall_ms = 80;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &SlowNoProgressDelegatedBrowse,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let stalls = journal
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|kind| kind.as_str() == "browser_budget_exceeded")
            .count();
        assert_eq!(
            stalls, 0,
            "the delegated browse no-progress terminalizes before the manager stall window"
        );
        assert_eq!(
            model.calls.load(Ordering::SeqCst),
            1,
            "the model does not get a normal post-browse turn or forced synthesis"
        );
        let crate::TurnStop::Failed { failure } = &outcome.stop else {
            panic!(
                "no-progress browse must fail the turn, got {:?}",
                outcome.stop
            );
        };
        assert_eq!(failure.code, "browser_structured_no_progress");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn browser_exhaustion_with_empty_synthesis_delivers_fallback_answer() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "browse" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = BrowserEmptyFinalModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.browser_budget.max_elapsed_ms = 300_000;
        turn_cfg.browser_budget.max_stall_ms = 80;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &SlowNoProgressDelegatedBrowse,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "browse".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_browser_failure_code(&outcome, "browser_structured_no_progress");
        assert!(
            outcome.memory_answer.contains("Non sono riuscito"),
            "{}",
            outcome.memory_answer
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn browser_exhaustion_with_grounded_partial_result_delivers_that_evidence() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "trova un treno Milano Roma" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = BrowserEmptyFinalModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.browser_budget.max_elapsed_ms = 300_000;
        turn_cfg.browser_budget.max_stall_ms = 80;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &GroundedPartialDelegatedBrowse,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "trova un treno Milano Roma".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_browser_failure_code(&outcome, "browser_structured_no_progress");
        assert!(
            outcome.memory_answer.contains("FR 9512"),
            "{}",
            outcome.memory_answer
        );
        assert!(
            outcome
                .memory_answer
                .contains("https://www.trenitalia.com/search"),
            "{}",
            outcome.memory_answer
        );
        assert!(
            !outcome.memory_answer.contains("Non sono riuscito"),
            "{}",
            outcome.memory_answer
        );
    }

    /// A manager-level delegated `browse` that SUCCEEDS every time, with a distinct goal per round
    /// (so the identical-call repeat guard never fires — repetition is a separate control).
    struct CountingDelegatedBrowse(std::sync::Arc<AtomicUsize>);

    impl CapabilityExecutor for CountingDelegatedBrowse {
        async fn execute_tool(
            &self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            _state: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(ToolOutcome {
                result: "found: true".to_string(),
                effects: ToolEffects {
                    browser_activity_observed: true,
                    outcome_hint: Some(crate::ToolOutcomeHint::Success),
                    ..ToolEffects::default()
                },
            })
        }
    }

    /// Keeps delegating `browse` forever, varying the goal so each round has a fresh signature.
    #[derive(Default)]
    struct EndlessBrowseManagerModel {
        calls: AtomicUsize,
    }

    impl ModelClient for EndlessBrowseManagerModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ModelRoundOutput {
                message: json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": format!("browse_{index}"),
                        "type": "function",
                        "function": {
                            "name": "browse",
                            "arguments": format!("{{\"goal\":\"phase {index}\"}}")
                        }
                    }]
                }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("tool_calls".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn progressing_manager_runs_past_the_round_cap_because_delegated_browses_reset_it() {
        // Manager-level counterpart of `progressing_browse_runs_past_the_round_cap_because_progress_
        // resets_it`. A real booking turn (search → choose → book) spends ONE multi-minute `browse`
        // per phase; every delegation succeeds. No granular browser tool ever runs in the manager
        // turn, so the granular progress reset cannot fire here — only `browser_activity_observed`
        // can. If that reset regressed, `progress_anchor_round` would stay pinned at 0 and the cap
        // would count TOTAL rounds, killing a manager that was progressing the whole way.
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "book a train" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let browses = std::sync::Arc::new(AtomicUsize::new(0));
        let executor = CountingDelegatedBrowse(browses.clone());
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        // Ceiling deliberately well ABOVE the round cap so "capped at the cap" and "ran past it" are
        // distinguishable outcomes.
        turn_cfg.hard_round_ceiling = 12;
        turn_cfg.max_rounds = 4;
        turn_cfg.browser_max_rounds = 4;
        turn_cfg.browser_subturn = false; // this is the MANAGER turn
        // Nothing time- or stagnation-based may interfere: only the round cap is on trial.
        turn_cfg.browser_budget.max_elapsed_ms = 600_000;
        turn_cfg.browser_budget.max_stall_ms = 600_000;
        turn_cfg.browser_budget.max_no_progress = 50;
        let round_cap = turn_cfg.browser_max_rounds;

        let _ = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &EndlessBrowseManagerModel::default(),
            &executor,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "book a train".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let ran = browses.load(Ordering::SeqCst);
        assert!(
            ran > round_cap,
            "a manager whose every delegated browse succeeds must run PAST browser_max_rounds \
             ({round_cap}), not be capped at it — only {ran} browses ran"
        );
    }

    // --- Finalization fence rework: drain / bounded-wait / park (steering-park-resume, Task 1) ---

    /// A tool ran while a steering row became interpreted as a plain `continue` — the disposition
    /// `wait_for_interrupting_control` (used mid-round/mid-tool) deliberately ignores, so it never
    /// surfaces there. The fence's drain step must pick it up via `current_turn_control()` directly
    /// (which returns ANY interpreted row, continue included) instead of spinning on it forever.
    #[derive(Default)]
    struct TrailingContinueAtFenceModel {
        calls: AtomicUsize,
        control_ready: AtomicBool,
        applied: AtomicBool,
        completed: AtomicBool,
    }

    impl ModelClient for TrailingContinueAtFenceModel {
        fn finalization_fence(&self) -> crate::FinalizationFence {
            if self.control_ready.load(Ordering::SeqCst) && !self.applied.load(Ordering::SeqCst) {
                crate::FinalizationFence::PendingInput
            } else {
                crate::FinalizationFence::Ready
            }
        }

        fn current_turn_control(&self) -> Option<crate::TurnControlDecision> {
            (self.control_ready.load(Ordering::SeqCst) && !self.applied.load(Ordering::SeqCst))
                .then(|| crate::TurnControlDecision {
                    steering_id: 11,
                    disposition: crate::TurnControlDisposition::ContinueCurrentWork,
                    instruction: "Keep going with the current step".to_string(),
                })
        }

        async fn wait_for_turn_control(&self) -> crate::TurnControlDecision {
            loop {
                if let Some(control) = self.current_turn_control() {
                    return control;
                }
                tokio::task::yield_now().await;
            }
        }

        fn acknowledge_turn_control_applied(&self, steering_id: i64) {
            assert_eq!(steering_id, 11);
            self.applied.store(true, Ordering::SeqCst);
        }

        fn acknowledge_turn_control_completed(&self, steering_id: i64) {
            assert_eq!(steering_id, 11);
            self.completed.store(true, Ordering::SeqCst);
        }

        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let message = if index == 0 {
                json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "browse_once",
                        "type": "function",
                        "function": { "name": "browse", "arguments": "{}" }
                    }]
                })
            } else {
                assert!(call.is_final_round);
                assert!(self.applied.load(Ordering::SeqCst));
                assert!(call.messages.iter().any(|message| {
                    message["content"]
                        .as_str()
                        .is_some_and(|content| content.contains("Keep going with the current step"))
                }));
                json!({"role": "assistant", "content": "Answer after the drained continue."})
            };
            Ok(ModelRoundOutput {
                message,
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some(if index == 0 { "tool_calls" } else { "stop" }.to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    struct ContinueSettingBrowseTool<'a>(&'a TrailingContinueAtFenceModel);

    impl CapabilityExecutor for ContinueSettingBrowseTool<'_> {
        async fn execute_tool(
            &self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            _state: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            self.0.control_ready.store(true, Ordering::SeqCst);
            Ok(ToolOutcome {
                result: "found: false".to_string(),
                effects: ToolEffects::default(),
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn trailing_continue_at_the_fence_is_drained_and_finalizes() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "research" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = TrailingContinueAtFenceModel::default();
        let tool = ContinueSettingBrowseTool(&model);
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 1;
        turn_cfg.max_rounds = 1;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &tool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "research".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        // The trailing `continue` sitting at the fence is drained (applied) rather than parked —
        // the turn finalizes via forced synthesis instead of hanging on the old spin.
        assert_eq!(outcome.stop, crate::TurnStop::Completed);
        assert!(model.applied.load(Ordering::SeqCst));
        assert!(model.completed.load(Ordering::SeqCst));
        assert_eq!(
            sink.0
                .lock()
                .unwrap()
                .iter()
                .filter(|event| matches!(event, GenerateStreamEvent::Done { .. }))
                .count(),
            1
        );
    }

    /// The fence stays `PendingInput` forever and `current_turn_control()` never returns an
    /// interpreted row (the steering stays `pending`/`claimed` — e.g. the semantic model is
    /// unavailable to interpret it). `wait_for_turn_control()` models the REAL production
    /// behavior (`GatewayModelClient::wait_for_turn_control`): it returns ONLY once a non-
    /// `continue` interpreted control appears, else it never resolves on its own — so with
    /// `current_turn_control()` permanently `None`, awaiting it directly would block forever.
    /// The reworked park loop must tick on a plain wall-clock sleep instead of this seam, so it
    /// parks within its bounded budget regardless. If a regression makes the loop await this
    /// future directly, the test hangs and the `tokio::time::timeout` guard below catches it.
    #[derive(Default)]
    struct NeverInterpretedSteeringModel {
        calls: AtomicUsize,
        wait_calls: AtomicUsize,
    }

    impl ModelClient for NeverInterpretedSteeringModel {
        fn finalization_fence(&self) -> crate::FinalizationFence {
            crate::FinalizationFence::PendingInput
        }

        fn current_turn_control(&self) -> Option<crate::TurnControlDecision> {
            None
        }

        async fn wait_for_turn_control(&self) -> crate::TurnControlDecision {
            self.wait_calls.fetch_add(1, Ordering::SeqCst);
            // Never resolves — mirrors production when the row never gets interpreted. The
            // fixed park loop must not depend on this future completing.
            std::future::pending().await
        }

        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ModelRoundOutput {
                message: json!({
                    "role": "assistant",
                    "content": "Draft answer while steering is still pending.",
                }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_turn_that_already_has_an_answer_delivers_instead_of_parking() {
        // Parking returns an empty answer and emits no terminal event, expecting a later coordinator
        // resume. When the work is already finished that is pure loss: observed in production as a
        // completed train search whose turn parked afterwards — the user had the full answer on
        // screen while the bubble span for 80 minutes, and the finished result was discarded.
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "investigate" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = NeverInterpretedSteeringModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 1;
        turn_cfg.max_rounds = 1;

        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            run_turn(
                ls,
                turn_cfg,
                &usage_context(),
                &model,
                &NoTools,
                &mut browser,
                &NoPlan,
                &DoneJudge,
                &NoCompact,
                &OpenPolicy,
                &journal,
                &sink,
                0.0,
                None,
                &std::collections::BTreeSet::new(),
                &[],
                "investigate".to_string(),
                // The turn already produced a complete answer before the fence blocked.
                "Ecco i treni: Frecciarossa 9524 alle 08:10.".to_string(),
                None,
                false,
                0,
                false,
                Vec::new(),
                None,
                &crate::turn_trace::TurnTrace::disabled(),
            ),
        )
        .await
        .expect("run_turn must not hang");

        assert_eq!(
            outcome.stop,
            crate::TurnStop::Completed,
            "a turn holding a finished answer must complete, not suspend it away"
        );
        assert!(
            outcome.memory_answer.contains("Frecciarossa 9524"),
            "the finished answer must survive: {:?}",
            outcome.memory_answer
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn uninterpreted_pending_steering_at_the_fence_parks_within_budget() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "investigate" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = NeverInterpretedSteeringModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 1;
        turn_cfg.max_rounds = 1;

        // Guard against a regression to the old infinite spin (or to a park loop that awaits
        // the blocking `wait_for_turn_control()` seam): fail fast with a clear panic instead of
        // hanging the whole test binary.
        let started = std::time::Instant::now();
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            run_turn(
                ls,
                turn_cfg,
                &usage_context(),
                &model,
                &NoTools,
                &mut browser,
                &NoPlan,
                &DoneJudge,
                &NoCompact,
                &OpenPolicy,
                &journal,
                &sink,
                0.0,
                None,
                &std::collections::BTreeSet::new(),
                &[],
                "investigate".to_string(),
                String::new(),
                None,
                false,
                0,
                false,
                Vec::new(),
                None,
                &crate::turn_trace::TurnTrace::disabled(),
            ),
        )
        .await
        .expect("run_turn must park within its bounded wait budget, not hang");
        let elapsed = started.elapsed();

        assert!(matches!(
            outcome.stop,
            crate::TurnStop::SuspendedModel { .. }
        ));
        assert!(outcome.memory_answer.is_empty());
        // Proves the budget is wall-clock-ticked (PARK_WAIT_CYCLES=40 * 50ms ~= 2s), not an
        // instant return and not a hang: real time actually elapsed, comfortably under the
        // 5s timeout above.
        assert!(
            elapsed >= std::time::Duration::from_millis(1_500),
            "expected the bounded wait to actually tick ~2s of wall-clock time, elapsed={elapsed:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "park must complete within its own budget, well before the outer timeout, elapsed={elapsed:?}"
        );
        // `wait_for_turn_control()` never resolves in this double (`std::future::pending()`), so
        // the ONLY way this line is reached at all is if the park loop's tick never awaited it
        // to completion — a direct await here would have hung and been caught by the timeout
        // above instead. The lone round-0 model call races the loop's OTHER (unrelated)
        // `wait_for_interrupting_control` select arm against `generate()`, which may poll (but
        // never complete) this same seam once — hence `<= 1`, not a strict `== 0`.
        assert!(
            model.wait_calls.load(Ordering::SeqCst) <= 1,
            "wait_for_turn_control should be polled at most once (the unrelated round-call race), \
             never awaited to completion by the park loop's tick"
        );
        assert_eq!(
            sink.0
                .lock()
                .unwrap()
                .iter()
                .filter(|event| matches!(event, GenerateStreamEvent::Done { .. }))
                .count(),
            0,
            "a parked turn must never emit a terminal Done"
        );
        // The park path must emit a resumable checkpoint at the boundary (the coordinator-resume
        // pipeline reseeds from it). At least the normal per-round checkpoint fires (round 0, the
        // only round `hard_round_ceiling = 1` allows) PLUS the park-point checkpoint itself — both
        // at round 0 here; assert on the LAST one (the park checkpoint) rather than an exact count,
        // since that's not this test's concern.
        let checkpoints = journal.1.lock().unwrap();
        assert!(
            !checkpoints.is_empty(),
            "expected at least one checkpoint emitted by the time the turn parks"
        );
        assert_eq!(
            checkpoints.last().unwrap().round,
            0,
            "the park checkpoint must capture the last round the loop actually ran"
        );
    }

    /// A steering control drained/applied EARLIER in the turn (round 0, via `current_turn_control`
    /// at the top of the round loop) must still be acknowledged completed even though the turn
    /// later parks on a DIFFERENT row that never becomes interpretable. Before the flush fix, the
    /// park early-return skipped the normal turn-end completion block entirely, silently dropping
    /// this id — its store row would be stuck at `applied` forever (a permanent "Applying…"
    /// spinner in the UI).
    #[derive(Default)]
    struct EarlierAppliedThenStuckSteeringModel {
        calls: AtomicUsize,
        early_control_taken: AtomicBool,
        early_applied: AtomicBool,
        early_completed: AtomicBool,
    }

    impl ModelClient for EarlierAppliedThenStuckSteeringModel {
        fn finalization_fence(&self) -> crate::FinalizationFence {
            // Always pending: a SECOND, different row is stuck uninterpreted at the fence
            // regardless of what happened earlier in the turn with the first row.
            crate::FinalizationFence::PendingInput
        }

        fn current_turn_control(&self) -> Option<crate::TurnControlDecision> {
            // Offered exactly once, at the very top of round 0 (before `early_control_taken`
            // flips) — a control drained by the ROUND LOOP itself, not the fence. Never
            // interpreted again afterward (the stuck row at the fence is a DIFFERENT id that
            // this double never surfaces through `current_turn_control`).
            (!self.early_control_taken.load(Ordering::SeqCst)).then(|| {
                self.early_control_taken.store(true, Ordering::SeqCst);
                crate::TurnControlDecision {
                    steering_id: 5,
                    disposition: crate::TurnControlDisposition::ContinueCurrentWork,
                    instruction: "Keep going — applied early in the turn".to_string(),
                }
            })
        }

        async fn wait_for_turn_control(&self) -> crate::TurnControlDecision {
            // Models production: never resolves once nothing is left to interpret.
            std::future::pending().await
        }

        fn acknowledge_turn_control_applied(&self, steering_id: i64) {
            assert_eq!(steering_id, 5);
            self.early_applied.store(true, Ordering::SeqCst);
        }

        fn acknowledge_turn_control_completed(&self, steering_id: i64) {
            assert_eq!(steering_id, 5);
            self.early_completed.store(true, Ordering::SeqCst);
        }

        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ModelRoundOutput {
                message: json!({
                    "role": "assistant",
                    "content": "Draft answer while a different row is still stuck pending.",
                }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn park_flushes_completion_for_steering_drained_earlier_in_the_turn() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "investigate" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = EarlierAppliedThenStuckSteeringModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 1;
        turn_cfg.max_rounds = 1;

        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            run_turn(
                ls,
                turn_cfg,
                &usage_context(),
                &model,
                &NoTools,
                &mut browser,
                &NoPlan,
                &DoneJudge,
                &NoCompact,
                &OpenPolicy,
                &journal,
                &sink,
                0.0,
                None,
                &std::collections::BTreeSet::new(),
                &[],
                "investigate".to_string(),
                String::new(),
                None,
                false,
                0,
                false,
                Vec::new(),
                None,
                &crate::turn_trace::TurnTrace::disabled(),
            ),
        )
        .await
        .expect("run_turn must park within its bounded wait budget, not hang");

        assert!(matches!(
            outcome.stop,
            crate::TurnStop::SuspendedModel { .. }
        ));
        assert!(model.early_applied.load(Ordering::SeqCst));
        // The regression: without the flush-before-park fix, this stays false — the id applied
        // earlier in the turn is silently dropped when the turn parks on the later, different,
        // uninterpreted row.
        assert!(
            model.early_completed.load(Ordering::SeqCst),
            "a steering control drained/applied earlier in the turn must still be acknowledged \
             completed when the turn parks on a later, different, uninterpreted row"
        );
    }

    /// A steering control interpreted as CLARIFY, surfacing while a tool is in flight — the same
    /// shape as `SteeringFinalizeModel`, but with the disposition that asks the USER a question.
    #[derive(Default)]
    struct SteeringClarifyModel {
        calls: AtomicUsize,
        control_ready: AtomicBool,
        applied: AtomicBool,
    }

    impl ModelClient for SteeringClarifyModel {
        fn current_turn_control(&self) -> Option<crate::TurnControlDecision> {
            (self.control_ready.load(Ordering::SeqCst) && !self.applied.load(Ordering::SeqCst))
                .then(|| crate::TurnControlDecision {
                    steering_id: 11,
                    disposition: crate::TurnControlDisposition::NeedsClarification,
                    instruction: "Which of the two accounts did you mean?".to_string(),
                })
        }

        fn acknowledge_turn_control_applied(&self, steering_id: i64) {
            assert_eq!(steering_id, 11);
            self.applied.store(true, Ordering::SeqCst);
        }

        async fn wait_for_turn_control(&self) -> crate::TurnControlDecision {
            std::future::poll_fn(|context| {
                if let Some(control) = self.current_turn_control() {
                    std::task::Poll::Ready(control)
                } else {
                    context.waker().wake_by_ref();
                    std::task::Poll::Pending
                }
            })
            .await
        }

        async fn generate(
            &self,
            call: &ModelCall<'_>,
            _on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let message = if index == 0 {
                json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "tool_wait",
                        "type": "function",
                        "function": { "name": "wait_forever", "arguments": "{}" }
                    }]
                })
            } else {
                json!({"role": "assistant", "content": "Which account did you mean?"})
            };
            Ok(ModelRoundOutput {
                message,
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some(if index == 0 { "tool_calls" } else { "stop" }.to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    struct PendingClarifyTool<'a>(&'a SteeringClarifyModel);

    impl CapabilityExecutor for PendingClarifyTool<'_> {
        async fn execute_tool(
            &self,
            _name: &str,
            _args: &str,
            _call_id: &str,
            _state: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            self.0.control_ready.store(true, Ordering::SeqCst);
            std::future::pending().await
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn clarify_steering_sets_awaiting_user_instead_of_looking_like_a_plain_finalize() {
        // Regression (triage MINOR 9): CLARIFY used to share the `break 'rounds` arm with
        // FINALIZE, so the caller saw an ordinary answer and could not tell that the turn
        // actually stopped to ask the user something.
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "move the money" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = SteeringClarifyModel::default();
        let tool = PendingClarifyTool(&model);
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;

        let outcome = run_turn(
            ls,
            cfg(),
            &usage_context(),
            &model,
            &tool,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "move the money".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let envelope = outcome
            .awaiting_user
            .expect("a CLARIFY steering must reach the caller as a typed wait");
        assert_eq!(envelope.kind, HitlKind::Clarify);
        assert_eq!(envelope.hold_policy, crate::hitl::HoldPolicy::Free);
    }

    /// Model that answers once with a valid CHOICES card while the plan still has open steps.
    /// If the harness wrongly nudges, a second generate() call happens.
    #[derive(Default)]
    struct ChoicesWhilePlanOpenModel {
        calls: AtomicUsize,
    }

    impl ModelClient for ChoicesWhilePlanOpenModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let content = if index == 0 {
                concat!(
                    "Please pick one.\n",
                    r#"‹‹CHOICES››{"question":"Which train?","options":["07:30","09:15"]}‹‹/CHOICES››"#,
                )
            } else {
                // Nudge path — must not run under the Turn Contract.
                "I continued without waiting for you."
            };
            on_delta(content);
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": content }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn choices_card_stops_the_turn_instead_of_nudging_an_open_plan() {
        // Turn Contract: CHOICES = AwaitingUser. An open plan must not trigger
        // answer_did_not_conclude_plan or forced_synthesis over the choice.
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "book a train" }),
        ];
        ls.step_messages_start = ls.messages.len();
        ls.plan = json!({
            "steps": [
                {"id":"s1","title":"Search","status":"done","detail":""},
                {"id":"s2","title":"Let the user choose","status":"doing","detail":""},
                {"id":"s3","title":"Book","status":"todo","detail":""},
            ]
        });
        let model = ChoicesWhilePlanOpenModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 6;
        turn_cfg.reconcile_on_delivery = false;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "book a train".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(
            model.calls.load(Ordering::SeqCst),
            1,
            "open-plan nudge must not call the model again after CHOICES"
        );
        assert_eq!(outcome.stop, crate::TurnStop::SuspendedUser);
        assert!(
            outcome.memory_answer.contains("‹‹CHOICES››"),
            "choice card must be delivered so the UI/gateway can park: {}",
            outcome.memory_answer
        );
        let events = journal.0.lock().unwrap().clone();
        assert!(
            !events.iter().any(|e| e.contains("ForcedSynthesis")),
            "forced synthesis must not run over AwaitingUser: {events:?}"
        );
    }

    struct LongBlockedAnswerModel;

    impl ModelClient for LongBlockedAnswerModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let content = format!(
                "‹‹PLAN››- [-] **Open the second source** (`s2`): pending‹‹/PLAN››\n\
                 The second source was not opened, so this step is still blocked. {}",
                "No evidence was produced for this step. ".repeat(30)
            );
            on_delta(&content);
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": content }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    struct ReconciledDeliveryAnswerModel;

    impl ModelClient for ReconciledDeliveryAnswerModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let content = "Il campo Text input contiene smoke sulla pagina Selenium.";
            on_delta(content);
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": content }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[derive(Default)]
    struct DeliveryReconcilePlan(Mutex<Vec<Vec<Value>>>);

    impl PlanProgress for DeliveryReconcilePlan {
        async fn persist_plan(
            &self,
            _thread_id: Option<&str>,
            _goal: Option<&str>,
            steps: &[Value],
        ) {
            self.0.lock().unwrap().push(steps.to_vec());
        }

        async fn record_step_outcome(
            &self,
            _thread_id: Option<&str>,
            _step: &Value,
            _evidence: &[String],
        ) {
        }

        async fn verify_step_complete(
            &self,
            _title: &str,
            _criterion: &str,
            _evidence: &str,
        ) -> (bool, String) {
            (false, String::new())
        }

        fn reconcile_on_delivery(&self, _plan: &Value, _delivered: &str) -> Option<Vec<Value>> {
            Some(vec![
                json!({"id":"execute_work","title":"Execute the requested work","status":"done","detail":"typed smoke"}),
                json!({"id":"verify_and_answer","title":"Verify and answer","status":"done","detail":"reported smoke"}),
            ])
        }

        fn plan_value_from_steps(&self, goal: Option<&str>, steps: &[Value]) -> Value {
            json!({"goal": goal, "steps": steps})
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn delivery_reconcile_emits_final_plan_update_event() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "fill the form" }),
        ];
        ls.step_messages_start = ls.messages.len();
        ls.plan = json!({
            "goal": "fill the form",
            "steps": [
                {"id":"execute_work","title":"Execute the requested work","status":"done","detail":"typed smoke"},
                {"id":"verify_and_answer","title":"Verify and answer","status":"doing","detail":"pending"},
            ]
        });
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let plan = DeliveryReconcilePlan::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 2;
        turn_cfg.reconcile_on_delivery = true;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &ReconciledDeliveryAnswerModel,
            &NoTools,
            &mut browser,
            &plan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "fill the form".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(outcome.stop, crate::TurnStop::Completed);
        let events = sink.0.lock().unwrap();
        let final_plan = events
            .iter()
            .rev()
            .find_map(|event| match event {
                GenerateStreamEvent::PlanUpdate { markdown } => Some(markdown),
                _ => None,
            })
            .expect("delivery reconcile must emit a final PlanUpdate event");
        assert!(
            final_plan.contains("- [x] **Verify and answer** (`verify_and_answer`)"),
            "final PlanUpdate must expose the reconciled completed plan, got: {final_plan}"
        );
        assert!(
            events
                .iter()
                .rposition(|event| matches!(event, GenerateStreamEvent::PlanUpdate { .. }))
                < events
                    .iter()
                    .rposition(|event| matches!(event, GenerateStreamEvent::Done { .. })),
            "the final plan update must be visible before Done closes the turn"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn substantial_blocked_answer_never_marks_an_unverified_step_done() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "open two sources" }),
        ];
        ls.step_messages_start = ls.messages.len();
        ls.plan = json!({
            "steps": [
                {"id":"s1","title":"Open the first source","status":"done","detail":"ok"},
                {"id":"s2","title":"Open the second source","status":"doing","detail":"pending"},
            ]
        });
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let plan = RecordingPlan::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 2;
        turn_cfg.reconcile_on_delivery = true;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &LongBlockedAnswerModel,
            &NoTools,
            &mut browser,
            &plan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "open two sources".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        let persisted = plan.0.lock().unwrap();
        assert!(persisted.iter().all(|steps| {
            steps.iter().all(|step| {
                step.get("id").and_then(Value::as_str) != Some("s2")
                    || plan_step_status(step) != "done"
            })
        }));
        let _ = outcome;
    }

    /// Model that answers once with a valid CLARIFY card while the plan still has open steps.
    #[derive(Default)]
    struct ClarifyWhilePlanOpenModel {
        calls: AtomicUsize,
    }

    impl ModelClient for ClarifyWhilePlanOpenModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let content = if index == 0 {
                concat!(
                    "Mi servono i tuoi dati.\n",
                    r#"‹‹CLARIFY››{"question":"Passenger details?","fields":["name","email","phone"]}‹‹/CLARIFY››"#,
                )
            } else {
                "I continued without waiting for you."
            };
            on_delta(content);
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": content }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn clarify_card_stops_the_turn_instead_of_nudging_an_open_plan() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "book a train" }),
        ];
        ls.step_messages_start = ls.messages.len();
        ls.plan = json!({
            "steps": [
                {"id":"s1","title":"Search","status":"done","detail":""},
                {"id":"s2","title":"Collect passenger data","status":"doing","detail":""},
                {"id":"s3","title":"Pay","status":"todo","detail":""},
            ]
        });
        let model = ClarifyWhilePlanOpenModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 6;
        turn_cfg.reconcile_on_delivery = true;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "book a train".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(
            model.calls.load(Ordering::SeqCst),
            1,
            "open-plan nudge must not call the model again after CLARIFY"
        );
        assert_eq!(outcome.stop, crate::TurnStop::SuspendedUser);
        assert!(
            outcome.memory_answer.contains("‹‹CLARIFY››"),
            "clarify card must be delivered: {}",
            outcome.memory_answer
        );
        let events = journal.0.lock().unwrap().clone();
        assert!(
            !events.iter().any(|e| e.contains("ForcedSynthesis")),
            "forced synthesis must not run over AwaitingUser(Clarify): {events:?}"
        );
    }

    /// Prose field request → one nudge to emit CLARIFY, then stop on the card.
    #[derive(Default)]
    struct ProseThenClarifyModel {
        calls: AtomicUsize,
    }

    impl ModelClient for ProseThenClarifyModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let content = if index == 0 {
                "Mi servono i tuoi dati:\n- Nome e Cognome\n- Email\n- Telefono\nAppena me li dai proseguo."
            } else {
                concat!(
                    "Mi servono i tuoi dati.\n",
                    r#"‹‹CLARIFY››{"question":"Dati passeggero?","fields":["name","email","phone"]}‹‹/CLARIFY››"#,
                )
            };
            on_delta(content);
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": content }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prose_field_request_nudges_once_then_awaits_clarify_card() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "continue booking" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = ProseThenClarifyModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 6;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "continue booking".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(
            model.calls.load(Ordering::SeqCst),
            2,
            "exactly one clarify-card nudge then stop"
        );
        assert!(
            outcome.memory_answer.contains("‹‹CLARIFY››"),
            "final answer must include CLARIFY: {}",
            outcome.memory_answer
        );
        let events = journal.0.lock().unwrap().clone();
        assert!(
            !events.iter().any(|e| e.contains("ForcedSynthesis")),
            "no forced synthesis after CLARIFY: {events:?}"
        );
    }

    /// Payment wait prose → one nudge to emit PAYMENT_APPROVAL, then stop on the hold card.
    #[derive(Default)]
    struct ProseThenPaymentApprovalModel {
        calls: AtomicUsize,
    }

    impl ModelClient for ProseThenPaymentApprovalModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let content = if index == 0 {
                "Payment Approval Card già presentata: attendo la tua decisione."
            } else {
                concat!(
                    "Fermato prima del pagamento.\n",
                    r#"‹‹PAYMENT_APPROVAL››{"snapshot":{"approval_id":"pay_smoke_1","merchant":"Stripe Elements Demo","domain":"checkout.stripe.dev","amount_minor":12196,"currency":"USD","product_summary":"Pure Glow Cream + The Pure Set","payment_method_label":"No payment method entered","checkout_fingerprint":"stripe-elements-demo-12196-usd"}}‹‹/PAYMENT_APPROVAL››"#,
                )
            };
            on_delta(content);
            Ok(ModelRoundOutput {
                message: json!({ "role": "assistant", "content": content }),
                provider: ProviderBinding {
                    model: call.model.to_string(),
                    base_url: call.base_url.to_string(),
                    api_key: None,
                },
                finish_reason: Some("stop".to_string()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prose_payment_wait_nudges_once_then_awaits_payment_card() {
        let mut ls = LoopState::new();
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "ask for payment approval" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = ProseThenPaymentApprovalModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;
        let mut turn_cfg = cfg();
        turn_cfg.hard_round_ceiling = 6;

        let outcome = run_turn(
            ls,
            turn_cfg,
            &usage_context(),
            &model,
            &NoTools,
            &mut browser,
            &NoPlan,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "ask for payment approval".to_string(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        assert_eq!(
            model.calls.load(Ordering::SeqCst),
            2,
            "exactly one payment-card nudge then stop"
        );
        assert_eq!(outcome.stop, crate::TurnStop::SuspendedApproval);
        assert_eq!(outcome.awaiting_user.unwrap().kind, HitlKind::Payment);
        assert!(
            outcome.memory_answer.contains("‹‹PAYMENT_APPROVAL››"),
            "final answer must include PAYMENT_APPROVAL: {}",
            outcome.memory_answer
        );
        let events = journal.0.lock().unwrap().clone();
        assert!(
            !events.iter().any(|e| e.contains("ForcedSynthesis")),
            "no forced synthesis after PAYMENT_APPROVAL: {events:?}"
        );
    }

    // ─── Task #8/#9: Replan nudge + consecutive failure tracking ───

    /// A tool executor that ALWAYS returns no-progress, simulating repeated failures.
    #[derive(Default)]
    struct AlwaysFailingTool;
    impl CapabilityExecutor for AlwaysFailingTool {
        async fn execute_tool(
            &self,
            name: &str,
            _a: &str,
            _c: &str,
            _s: &mut LoopState,
        ) -> Result<ToolOutcome, String> {
            Ok(ToolOutcome {
                result: format!("{name} failed: no usable result"),
                effects: ToolEffects {
                    outcome_hint: Some(crate::contract::ToolOutcomeHint::NoProgress),
                    ..ToolEffects::default()
                },
            })
        }
    }

    /// A model that keeps calling the same tool (same family), simulating a stuck agent.
    /// After `max_calls` it gives a final text answer.
    #[derive(Default)]
    struct RepeatingToolModel {
        calls: AtomicUsize,
        max_calls: usize,
        captured_messages: Mutex<Vec<Vec<Value>>>,
    }
    impl ModelClient for RepeatingToolModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, crate::contract::ModelCallError> {
            let idx = self.calls.fetch_add(1, Ordering::SeqCst);
            self.captured_messages
                .lock()
                .unwrap()
                .push(call.messages.to_vec());
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if idx < self.max_calls {
                return Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": format!("call_{idx}"),
                            "type": "function",
                            "function": {
                                "name": "search_web",
                                "arguments": "{\"query\":\"test\"}"
                            }
                        }]
                    }),
                    provider,
                    finish_reason: Some("tool_calls".into()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                });
            }
            on_delta("Final answer after failures.");
            Ok(ModelRoundOutput {
                message: json!({
                    "role": "assistant",
                    "content": "Final answer after failures."
                }),
                provider,
                finish_reason: Some("stop".into()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    /// A model that alternates between DIFFERENT tool families on each call,
    /// simulating cross-family consecutive failures.
    #[derive(Default)]
    struct CrossFamilyFailingModel {
        calls: AtomicUsize,
        captured_messages: Mutex<Vec<Vec<Value>>>,
    }
    impl ModelClient for CrossFamilyFailingModel {
        async fn generate(
            &self,
            call: &ModelCall<'_>,
            on_delta: &(dyn Fn(&str) + Send + Sync),
        ) -> Result<ModelRoundOutput, crate::contract::ModelCallError> {
            let idx = self.calls.fetch_add(1, Ordering::SeqCst);
            self.captured_messages
                .lock()
                .unwrap()
                .push(call.messages.to_vec());
            let provider = ProviderBinding {
                model: call.model.to_string(),
                base_url: call.base_url.to_string(),
                api_key: None,
            };
            if idx < 4 {
                // Alternate between two different tool families so each family's
                // per-family counter never reaches 3, but the cross-family counter does.
                let tool_name = if idx.is_multiple_of(2) {
                    "search_web"
                } else {
                    "write_file"
                };
                return Ok(ModelRoundOutput {
                    message: json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": format!("call_{idx}"),
                            "type": "function",
                            "function": {
                                "name": tool_name,
                                "arguments": "{}"
                            }
                        }]
                    }),
                    provider,
                    finish_reason: Some("tool_calls".into()),
                    usage: Default::default(),
                    latency_ms: None,
                    time_to_first_token_ms: None,
                });
            }
            on_delta("Final cross-family answer.");
            Ok(ModelRoundOutput {
                message: json!({
                    "role": "assistant",
                    "content": "Final cross-family answer."
                }),
                provider,
                finish_reason: Some("stop".into()),
                usage: Default::default(),
                latency_ms: None,
                time_to_first_token_ms: None,
            })
        }
    }

    /// A PlanProgress that provides pre-seeded plan steps (so the replan directive
    /// can reference a real step title).
    #[derive(Default)]
    struct SeededPlan {
        _steps: Vec<Value>,
    }
    impl crate::contract::PlanProgress for SeededPlan {
        async fn persist_plan(&self, _t: Option<&str>, _g: Option<&str>, _s: &[Value]) {}
        async fn record_step_outcome(&self, _t: Option<&str>, _s: &Value, _e: &[String]) {}
        async fn verify_step_complete(
            &self,
            _title: &str,
            _criterion: &str,
            _evidence: &str,
        ) -> (bool, String) {
            (false, String::new())
        }
        fn reconcile_on_delivery(&self, _plan: &Value, _delivered: &str) -> Option<Vec<Value>> {
            None
        }
        fn plan_value_from_steps(&self, goal: Option<&str>, steps: &[Value]) -> Value {
            json!({"goal": goal, "steps": steps})
        }
    }

    fn seeded_plan_with_steps(steps: Vec<Value>) -> (SeededPlan, Value) {
        let plan = SeededPlan {
            _steps: steps.clone(),
        };
        let plan_value = json!({"steps": steps});
        (plan, plan_value)
    }

    fn higher_budget_cfg() -> TurnConfig {
        TurnConfig {
            hard_round_ceiling: 12,
            max_rounds: 8,
            ..cfg()
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn replan_nudge_injected_when_same_family_stalls() {
        let mut ls = LoopState::new();
        let (plan_progress, plan_value) = seeded_plan_with_steps(vec![
            json!({"id":"s1","title":"Search for data","status":"doing","detail":""}),
            json!({"id":"s2","title":"Write report","status":"todo","detail":""}),
        ]);
        ls.plan = plan_value;
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "find data and write report" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = RepeatingToolModel {
            max_calls: 5,
            ..Default::default()
        };
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;

        let _outcome = run_turn(
            ls,
            higher_budget_cfg(),
            &usage_context(),
            &model,
            &AlwaysFailingTool,
            &mut browser,
            &plan_progress,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "find data and write report".into(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        // Verify a replan system message was injected into the conversation.
        let captured = model.captured_messages.lock().unwrap();
        let has_replan_msg = captured.iter().any(|msgs| {
            msgs.iter().any(|m| {
                m.get("role").and_then(|r| r.as_str()) == Some("system")
                    && m.get("content")
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains("blocked after multiple attempts"))
                        .unwrap_or(false)
            })
        });
        assert!(
            has_replan_msg,
            "expected a replan system directive when same-family tool stalls"
        );
        // The model must have been called MORE than 3 times (replan injected → loop continued).
        assert!(
            model.calls.load(Ordering::SeqCst) > 3,
            "replan nudge should let the loop continue, got {} calls",
            model.calls.load(Ordering::SeqCst)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forced_replan_injected_on_cross_family_consecutive_failures() {
        let mut ls = LoopState::new();
        let (plan_progress, plan_value) = seeded_plan_with_steps(vec![
            json!({"id":"s1","title":"Gather info","status":"doing","detail":""}),
        ]);
        ls.plan = plan_value;
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "do a complex task" }),
        ];
        ls.step_messages_start = ls.messages.len();
        let model = CrossFamilyFailingModel::default();
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;

        let _outcome = run_turn(
            ls,
            higher_budget_cfg(),
            &usage_context(),
            &model,
            &AlwaysFailingTool,
            &mut browser,
            &plan_progress,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "do a complex task".into(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        // Verify a forced replan message was injected (cross-family).
        let captured = model.captured_messages.lock().unwrap();
        let has_forced_replan = captured.iter().any(|msgs| {
            msgs.iter().any(|m| {
                m.get("role").and_then(|r| r.as_str()) == Some("system")
                    && m.get("content")
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains("consecutive steps"))
                        .unwrap_or(false)
            })
        });
        assert!(
            has_forced_replan,
            "expected a forced replan message on cross-family consecutive failures"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn replan_nudge_fires_only_once_per_turn() {
        let mut ls = LoopState::new();
        let (plan_progress, plan_value) = seeded_plan_with_steps(vec![
            json!({"id":"s1","title":"Search for data","status":"doing","detail":""}),
        ]);
        ls.plan = plan_value;
        ls.messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "user", "content": "find data" }),
        ];
        ls.step_messages_start = ls.messages.len();
        // Model keeps calling tools even after the replan — the second stall must NOT
        // inject another replan (the flag prevents it).
        let model = RepeatingToolModel {
            max_calls: 8,
            ..Default::default()
        };
        let sink = Collect::default();
        let journal = CollectJournal::default();
        let mut browser = NoBrowser;

        let _outcome = run_turn(
            ls,
            higher_budget_cfg(),
            &usage_context(),
            &model,
            &AlwaysFailingTool,
            &mut browser,
            &plan_progress,
            &DoneJudge,
            &NoCompact,
            &OpenPolicy,
            &journal,
            &sink,
            0.0,
            None,
            &std::collections::BTreeSet::new(),
            &[],
            "find data".into(),
            String::new(),
            None,
            false,
            0,
            false,
            Vec::new(),
            None,
            &crate::turn_trace::TurnTrace::disabled(),
        )
        .await;

        // Count replan system messages in the LAST captured message snapshot (which
        // is the most complete view of the conversation). Each snapshot is cumulative,
        // so only the last one tells us the total count of replan directives injected.
        let captured = model.captured_messages.lock().unwrap();
        let last_snapshot = captured.last().expect("at least one model call");
        let replan_count = last_snapshot
            .iter()
            .filter(|m| {
                m.get("role").and_then(|r| r.as_str()) == Some("system")
                    && m.get("content")
                        .and_then(|c| c.as_str())
                        .map(|c| {
                            c.contains("blocked after multiple attempts")
                                || c.contains("consecutive steps")
                        })
                        .unwrap_or(false)
            })
            .count();
        assert!(
            replan_count <= 1,
            "replan must fire at most once per turn, found {replan_count}"
        );
        // Verify at least one replan WAS injected.
        assert_eq!(replan_count, 1, "expected exactly one replan directive");
    }
}
