//! Browser snapshot and screenshot context management.
//!
//! Manages browser-related messages in the conversation context:
//! screenshot injection, snapshot supersession, autocomplete detection,
//! and follow-up policy hints for form interactions.

use crate::provider::ChatMessage;

/// Extract autocomplete options (listbox/option elements) from a snapshot output.
/// Returns a short text summarizing available suggestions.
pub(crate) fn extract_autocomplete_suggestions(snapshot_output: &str) -> Option<String> {
    let mut suggestions = Vec::new();
    for line in snapshot_output.lines() {
        let trimmed = line.trim().trim_start_matches("- ");
        if trimmed.starts_with("option ") && trimmed.contains("[ref=") {
            suggestions.push(trimmed.to_string());
        }
    }
    if suggestions.is_empty() {
        return None;
    }
    let mut result = format!(
        "\n\nAutocomplete dropdown appeared with {} suggestion(s):\n",
        suggestions.len()
    );
    for s in suggestions.iter().take(10) {
        result.push_str("  - ");
        result.push_str(s);
        result.push('\n');
    }
    result.push_str(
        "→ Click the matching option to select it (e.g. playwright__browser_click with ref=\"eN\")",
    );
    Some(result)
}

/// Extract screenshot file paths from browser tool output.
pub(crate) fn extract_browser_screenshot_paths(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| line.trim().strip_prefix("📁 File: "))
        .map(str::trim)
        .filter(|path| {
            let lower = path.to_ascii_lowercase();
            lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
        })
        .map(ToString::to_string)
        .collect()
}

/// Build a user message containing a browser screenshot for model vision context.
pub(crate) fn build_browser_screenshot_context_message(
    tool_output: &str,
    capabilities: &crate::config::ModelCapabilities,
) -> Option<ChatMessage> {
    if !(capabilities.multimodal || capabilities.image_input) {
        return None;
    }

    let screenshot_path = extract_browser_screenshot_paths(tool_output).pop()?;
    let is_form_map = tool_output.contains("FORM MAP");

    let label = if is_form_map {
        // Persistent form map — stays until page navigation.
        // Extract the FORM MAP legend to include alongside the image.
        let legend = tool_output
            .lines()
            .skip_while(|l| !l.starts_with("FORM MAP"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "Form field reference map — numbered labels on the screenshot show where each \
             field is located. Use this to verify you are targeting the correct ref before \
             each type/fill action.\n\n{legend}"
        )
    } else {
        // Temporary control screenshot — cleared before next LLM turn.
        "Temporary browser screenshot. Inspect this visual state together with the \
         browser snapshot/tool result before deciding the next action."
            .to_string()
    };

    Some(ChatMessage::user_parts(vec![
        crate::provider::ChatContentPart::Text { text: label },
        crate::provider::ChatContentPart::Image {
            path: screenshot_path,
            media_type: "image/png".to_string(),
        },
    ]))
}

/// Generate follow-up policy instructions based on browser form state.
///
/// Injects checklist reminders when the page shows autocomplete dropdowns,
/// date/time pickers, or blocked form submissions.
pub(crate) fn browser_follow_up_instruction(tool_output: &str) -> Option<String> {
    let trimmed = tool_output.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    let blocked_submit = lower.contains("blocked click on element [")
        && lower.contains("form still looks incomplete");
    let has_suggestions = lower.contains("visible suggestions:");
    let has_autocomplete = lower.contains("autocomplete/combobox")
        || lower.contains("autocomplete still open")
        || lower.contains("combobox-style field");
    let has_date_picker = lower.contains("date picker appears to be open");
    let has_time_picker = lower.contains("time options appear to be open");

    if !(blocked_submit
        || has_suggestions
        || has_autocomplete
        || has_date_picker
        || has_time_picker)
    {
        return None;
    }

    let mut checklist = Vec::new();
    if has_suggestions || has_autocomplete {
        checklist.push(
            "If a field shows suggestions or behaves like a combobox, explicitly select the visible option before moving on."
                .to_string(),
        );
    }
    if has_date_picker {
        checklist.push(
            "If the site opened a date picker, choose the requested date from the picker and verify the field updated."
                .to_string(),
        );
    }
    if has_time_picker {
        checklist.push(
            "If the site opened time options, select the requested departure/arrival time explicitly."
                .to_string(),
        );
    }
    if blocked_submit {
        checklist.push(
            "Do not attempt to submit again until all required visible fields are confirmed and no autocomplete/picker is left unresolved."
                .to_string(),
        );
    }
    checklist.push(
        "After each field change, inspect the updated browser snapshot before deciding the next action."
            .to_string(),
    );

    Some(format!(
        "Browser form policy reminder based on the latest page state:\n- {}",
        checklist.join("\n- ")
    ))
}

/// Remove temporary browser screenshots from context, deleting files from disk.
pub(crate) fn clear_temporary_browser_screenshot_context(messages: &mut Vec<ChatMessage>) {
    // Delete screenshot files from disk before removing the messages.
    for msg in messages.iter() {
        if let Some(path) = temporary_browser_screenshot_path_from_message(msg) {
            let _ = std::fs::remove_file(&path);
        }
    }
    messages.retain(|message| !is_temporary_browser_screenshot_message(message));
}

/// Returns `true` if this message is a tool result from a browser snapshot action.
///
/// With the unified browser tool, snapshot results come from tool named `"browser"`
/// and contain the compacted accessibility tree with "interactive" count.
pub(crate) fn is_browser_snapshot_tool_result(msg: &ChatMessage) -> bool {
    if msg.role != "tool" {
        return false;
    }
    let is_browser = msg.name.as_deref() == Some("browser");
    if !is_browser {
        return false;
    }
    // Distinguish snapshot results from other browser actions by content markers.
    // Snapshot output contains "interactive elements)" — other actions don't.
    msg.content
        .as_deref()
        .is_some_and(|c| c.contains("interactive elements)"))
}

/// Returns `true` if this is an injected browser form policy reminder.
pub(crate) fn is_browser_follow_up_policy(msg: &ChatMessage) -> bool {
    msg.role == "user"
        && msg
            .content
            .as_deref()
            .is_some_and(|c| c.starts_with("Browser form policy reminder"))
}

/// After a new `browser_snapshot` tool result is pushed, replace all older
/// snapshot results with a compact one-line summary and remove stale
/// follow-up policy messages.
///
/// This keeps the model focused on the **current** page state rather than
/// accumulating 6K chars per snapshot × N iterations.
pub(crate) fn supersede_stale_browser_context(messages: &mut Vec<ChatMessage>) {
    // Collect indices of ALL browser snapshot tool results
    let snapshot_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, msg)| is_browser_snapshot_tool_result(msg))
        .map(|(i, _)| i)
        .collect();

    // Need at least 2 snapshots for there to be "stale" ones
    if snapshot_indices.len() < 2 {
        return;
    }

    // All but the last are stale — replace their content in-place
    // (preserving tool_call_id and name to keep the assistant↔tool chain valid)
    for &idx in &snapshot_indices[..snapshot_indices.len() - 1] {
        let summary =
            build_snapshot_superseded_summary(messages[idx].content.as_deref().unwrap_or(""));
        messages[idx].content = Some(summary);
        // Also clear any content_parts (could contain large data)
        messages[idx].content_parts = None;
    }

    // Collect indices of stale items to remove (screenshot images + follow-up policies).
    // Keep only the most recent of each. Remove from end to avoid index shifts.
    let mut indices_to_remove: Vec<usize> = Vec::new();

    // Stale temporary browser screenshot messages (images)
    let screenshot_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, msg)| is_temporary_browser_screenshot_message(msg))
        .map(|(i, _)| i)
        .collect();
    if screenshot_indices.len() > 1 {
        // Delete the screenshot file from disk before removing from context
        for &idx in &screenshot_indices[..screenshot_indices.len() - 1] {
            if let Some(path) = temporary_browser_screenshot_path_from_message(&messages[idx]) {
                let _ = std::fs::remove_file(&path);
            }
            indices_to_remove.push(idx);
        }
    }

    // Stale form map screenshots — keep only the most recent one
    let form_map_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, msg)| is_form_map_screenshot_message(msg))
        .map(|(i, _)| i)
        .collect();
    if form_map_indices.len() > 1 {
        for &idx in &form_map_indices[..form_map_indices.len() - 1] {
            if let Some(path) = form_map_screenshot_path_from_message(&messages[idx]) {
                let _ = std::fs::remove_file(&path);
            }
            indices_to_remove.push(idx);
        }
    }

    // Stale follow-up policy messages
    let policy_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, msg)| is_browser_follow_up_policy(msg))
        .map(|(i, _)| i)
        .collect();
    if policy_indices.len() > 1 {
        for &idx in &policy_indices[..policy_indices.len() - 1] {
            indices_to_remove.push(idx);
        }
    }

    // Remove collected indices from end to start
    indices_to_remove.sort_unstable();
    indices_to_remove.dedup();
    for &idx in indices_to_remove.iter().rev() {
        messages.remove(idx);
    }
}

/// Build a short one-line summary for a superseded browser snapshot.
fn build_snapshot_superseded_summary(snapshot_content: &str) -> String {
    let url = snapshot_content
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("Page URL: ")
                .or_else(|| line.trim().strip_prefix("page url: "))
        })
        .unwrap_or("unknown");

    let interactive_count = snapshot_content
        .lines()
        .filter(|line| line.contains("[ref="))
        .count();

    format!(
        "[Previous snapshot superseded — page: {url}, {interactive_count} interactive elements]"
    )
}

/// Returns `true` for **temporary** (control) browser screenshots.
/// Form map screenshots are NOT temporary — they persist until navigation.
pub(crate) fn is_temporary_browser_screenshot_message(message: &ChatMessage) -> bool {
    let Some(parts) = &message.content_parts else {
        return false;
    };
    parts.iter().any(|part| {
        matches!(
            part,
            crate::provider::ChatContentPart::Text { text }
                if text.starts_with("Temporary browser screenshot.")
        )
    })
}

/// Returns `true` for **persistent** form map screenshots (labeled overlay).
/// These are cleared on page navigation, not every LLM turn.
fn is_form_map_screenshot_message(message: &ChatMessage) -> bool {
    let Some(parts) = &message.content_parts else {
        return false;
    };
    parts.iter().any(|part| {
        matches!(
            part,
            crate::provider::ChatContentPart::Text { text }
                if text.starts_with("Form field reference map")
        )
    })
}

fn temporary_browser_screenshot_path_from_message(message: &ChatMessage) -> Option<String> {
    if !is_temporary_browser_screenshot_message(message) {
        return None;
    }
    screenshot_path_from_parts(message)
}

fn form_map_screenshot_path_from_message(message: &ChatMessage) -> Option<String> {
    if !is_form_map_screenshot_message(message) {
        return None;
    }
    screenshot_path_from_parts(message)
}

fn screenshot_path_from_parts(message: &ChatMessage) -> Option<String> {
    message
        .content_parts
        .as_ref()?
        .iter()
        .find_map(|part| match part {
            crate::provider::ChatContentPart::Image { path, .. } => Some(path.clone()),
            _ => None,
        })
}
