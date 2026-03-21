//! Iteration budget management for the agent loop.
//!
//! Tracks stall detection, cycle detection, and dynamic budget
//! extension/contraction to prevent infinite loops while allowing
//! complex multi-step tasks to complete.

/// Summary of a single tool execution within one iteration.
#[derive(Debug, Clone)]
pub(crate) struct ToolExecutionSummary {
    pub name: String,
    pub signature: String,
    pub useful: bool,
}

/// Mutable state for the iteration budget manager.
///
/// Tracks stall streaks, cycle signatures, and budget extensions
/// across iterations of the agent loop.
#[derive(Debug, Default)]
pub(crate) struct IterationBudgetState {
    pub(crate) last_signature: Option<String>,
    pub(crate) stall_streak: u8,
    pub(crate) extensions_used: u8,
    /// Rolling window of recent tool-call signatures for cycle detection.
    pub(crate) recent_signatures: Vec<String>,
    /// When a cycle is detected, stores the period (1 = same call repeated,
    /// 2 = A→B→A→B, 3 = A→B→C→A→B→C). Consumed by hint injection.
    pub(crate) cycle_detected: Option<usize>,
}

/// Build a deterministic signature for a tool call (name + serialized args).
pub(crate) fn tool_call_signature(tool_name: &str, arguments: &serde_json::Value) -> String {
    let args = serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string());
    format!("{tool_name}:{args}")
}

/// Evaluate tool results and adjust the iteration budget.
///
/// Extends the budget when the model is making progress (useful, non-repeated
/// tool calls) and contracts it when stalling or cycling.
pub(crate) fn maybe_extend_iteration_budget(
    active_budget: &mut u32,
    hard_max_iterations: u32,
    base_max_iterations: u32,
    iteration: u32,
    tool_summaries: &[ToolExecutionSummary],
    state: &mut IterationBudgetState,
    loop_detection_window: u8,
) {
    if tool_summaries.is_empty() {
        state.stall_streak = state.stall_streak.saturating_add(1);
        // Active contraction: if model stalls too long, cut the budget short.
        if state.stall_streak >= 4 && *active_budget > iteration + 2 {
            *active_budget = iteration + 2;
            tracing::warn!(
                iteration,
                active_budget = *active_budget,
                stall_streak = state.stall_streak,
                "Contracted iteration budget — model is stalling (empty tool calls)"
            );
        }
        return;
    }

    let signature = tool_summaries
        .iter()
        .map(|summary| summary.signature.as_str())
        .collect::<Vec<_>>()
        .join("|");
    let useful = tool_summaries.iter().any(|summary| summary.useful);
    let repeated_signature = state.last_signature.as_deref() == Some(signature.as_str());

    if useful && !repeated_signature {
        state.stall_streak = 0;
    } else {
        state.stall_streak = state.stall_streak.saturating_add(1);
    }
    state.last_signature = Some(signature.clone());

    // AB-1: Rolling window cycle detection.
    if loop_detection_window > 0 {
        state.recent_signatures.push(signature.clone());
        let win = loop_detection_window as usize;
        if state.recent_signatures.len() > win {
            let excess = state.recent_signatures.len() - win;
            state.recent_signatures.drain(..excess);
        }

        // Try exact match first, then fuzzy (normalized).
        let cycle = detect_cycle(&state.recent_signatures).or_else(|| {
            let normalized: Vec<String> = state
                .recent_signatures
                .iter()
                .map(|s| normalize_signature_for_cycle(s))
                .collect();
            detect_cycle(&normalized)
        });

        if let Some(period) = cycle {
            state.cycle_detected = Some(period);
            // Contract budget when cycling + some stall evidence.
            if state.stall_streak >= 2 && *active_budget > iteration + 2 {
                *active_budget = iteration + 2;
                tracing::warn!(
                    iteration,
                    active_budget = *active_budget,
                    cycle_period = period,
                    "Contracted iteration budget — cycle detected (period {})",
                    period,
                );
                return;
            }
        }
    }

    // Active contraction: if stalling for 4+ rounds, cut the budget to
    // current iteration + 2 so the model has a last chance then stops.
    if state.stall_streak >= 4 && *active_budget > iteration + 2 {
        *active_budget = iteration + 2;
        tracing::warn!(
            iteration,
            active_budget = *active_budget,
            stall_streak = state.stall_streak,
            "Contracted iteration budget — model is repeating the same actions"
        );
        return;
    }

    // Don't extend if: stalling, not useful, or repeating the same actions.
    // Repeated signatures mean no progress — extending would just waste tokens.
    if state.stall_streak >= 3 || !useful || repeated_signature {
        return;
    }

    if iteration + 1 < *active_budget {
        return;
    }

    let browser_heavy = tool_summaries
        .iter()
        .any(|summary| crate::browser::is_browser_tool(&summary.name));
    let search_heavy = tool_summaries
        .iter()
        .any(|summary| matches!(summary.name.as_str(), "web_search" | "web_fetch"));
    let extension = if browser_heavy {
        10
    } else if search_heavy {
        4
    } else {
        3
    };

    let next_budget = (*active_budget + extension)
        .max(base_max_iterations)
        .min(hard_max_iterations);
    if next_budget > *active_budget {
        *active_budget = next_budget;
        state.extensions_used = state.extensions_used.saturating_add(1);
        tracing::info!(
            iteration,
            active_budget = *active_budget,
            hard_max_iterations,
            browser_heavy,
            search_heavy,
            "Extended iteration budget after observing continued progress"
        );
    }
}

// ── AB-1: Cycle detection helpers ───────────────────────────────

/// Check the most recent signatures for repeating cycles of period 1, 2, or 3.
///
/// Returns the shortest detected period, or `None` if no cycle is found.
/// For period P we need at least 2*P entries and check that
/// `sigs[len-i] == sigs[len-i-P]` for `i` in `0..P`.
pub(crate) fn detect_cycle(signatures: &[String]) -> Option<usize> {
    let len = signatures.len();
    for period in 1..=3 {
        if len < 2 * period {
            continue;
        }
        let is_cycle =
            (0..period).all(|i| signatures[len - 1 - i] == signatures[len - 1 - i - period]);
        if is_cycle {
            return Some(period);
        }
    }
    None
}

/// Coarsen a composite signature for fuzzy cycle detection.
///
/// `web_search:{query}` and `web_fetch:{url}` are collapsed to just the tool
/// name, so queries with different parameters are treated as the same action.
/// All other tool segments are preserved verbatim.
pub(crate) fn normalize_signature_for_cycle(sig: &str) -> String {
    sig.split('|')
        .map(|segment| {
            let tool_name = segment.split(':').next().unwrap_or(segment);
            if matches!(tool_name, "web_search" | "web_fetch") {
                tool_name.to_string()
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("|")
}
