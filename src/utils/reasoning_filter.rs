//! Reasoning filter - removes thinking/reasoning blocks from LLM responses.
//!
//! Many modern LLMs include reasoning in their responses:
//! - DeepSeek R1: `<thinkThinking` sections
//! - `|Thinking|` blocks
//! - DeepSeek style thinking lines
//!
//! This module provides utilities to strip reasoning from responses
//! before sending to text-based channels (Telegram, WhatsApp, CLI).

use regex::Regex;
use std::sync::LazyLock;

/// Regex for think tags
static THINKING_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<think[^>]*>.*?</think\s*>").expect("Invalid thinking tag regex")
});

/// Remove reasoning/thinking blocks from text.
///
/// Strips:
/// - `<thinkThinking` sections
/// - `|Thinking|` blocks
/// - DeepSeek style thinking lines
///
/// Returns clean text suitable for text channels.
pub fn strip_reasoning(text: &str) -> String {
    let mut result = text.to_string();

    // Remove think tag blocks
    result = THINKING_TAG_REGEX.replace_all(&result, "").to_string();

    // Remove ## Thinking / ## Reasoning sections (manual parsing)
    result = strip_thinking_sections(&result);

    // Remove |Thinking| blocks
    result = strip_pipe_thinking(&result);

    // Remove DeepSeek R1 style thinking lines
    result = strip_deepseek_reasoning(&result);

    // Clean up excessive whitespace
    clean_whitespace(&result)
}

/// Strip ## Thinking / ## Reasoning sections manually.
fn strip_thinking_sections(text: &str) -> String {
    let thinking_headers = [
        "## Thinking", "## Reasoning", "## Thought", "## Ragionamento",
        "### Thinking", "### Reasoning", "### Thought", "### Ragionamento",
        "# Thinking", "# Reasoning", "# Thought", "# Ragionamento",
    ];

    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();
    let mut in_thinking = false;

    for line in lines {
        let trimmed = line.trim();

        // Check if this line starts a thinking section
        if thinking_headers.iter().any(|h| trimmed.starts_with(h)) {
            in_thinking = true;
            continue;
        }

        // Check if we've hit a new section (## Something else)
        if in_thinking && (trimmed.starts_with("##") || trimmed.starts_with("# ")) {
            // Make sure it's not another thinking section
            if !thinking_headers.iter().any(|h| trimmed.starts_with(h)) {
                in_thinking = false;
            }
        }

        if !in_thinking {
            result.push(line);
        }
    }

    result.join("\n")
}

/// Strip |Thinking| blocks.
fn strip_pipe_thinking(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();
    let mut in_pipe_block = false;

    for line in lines {
        let trimmed = line.trim();

        // Start of |Thinking| block
        if trimmed.starts_with("|Thinking") && trimmed.contains("|") {
            in_pipe_block = true;
            continue;
        }

        // End of pipe block (non-table line)
        if in_pipe_block && !trimmed.starts_with("|") {
            in_pipe_block = false;
        }

        if !in_pipe_block {
            result.push(line);
        }
    }

    result.join("\n")
}

/// Remove DeepSeek R1 style reasoning blocks.
fn strip_deepseek_reasoning(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    if lines.is_empty() {
        return text.to_string();
    }

    // Check if first line looks like a reasoning marker
    let first = lines[0].trim();
    if is_thinking_start_marker(first) {
        // Find the first line that doesn't look like thinking
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Skip empty lines and thinking lines
            if trimmed.is_empty() || is_thinking_line(trimmed) {
                continue;
            }

            // Found actual content - return from here
            if i > 0 {
                return lines[i..].join("\n");
            }
            break;
        }
    }

    text.to_string()
}

/// Check if a line is a thinking start marker.
fn is_thinking_start_marker(line: &str) -> bool {
    let markers = [
        "Hmm,", "Let me think", "Okay,", "Alright,", "Wait,",
        "Thinking...", "Reasoning...",
    ];
    markers.iter().any(|m| line.starts_with(m))
}

/// Check if a line looks like thinking/reasoning text.
fn is_thinking_line(line: &str) -> bool {
    let thinking_markers = [
        "let me think",
        "i need to",
        "first, i should",
        "the user wants",
        "looking at this",
        "considering",
        "hmm,",
        "wait,",
        "actually,",
        "i see that",
        "based on",
        "it seems",
        "the question",
        "to answer this",
        "per rispondere",
        "devo pensare",
        "lasciami pensare",
        "analizziamo",
        "dunque",
    ];

    let lower = line.to_lowercase();
    thinking_markers.iter().any(|marker| lower.starts_with(marker))
}

/// Clean up excessive whitespace from filtered text.
fn clean_whitespace(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();
    let mut prev_empty = false;

    for line in lines {
        let is_empty = line.trim().is_empty();

        // Skip consecutive empty lines (keep max 2)
        if is_empty && prev_empty {
            continue;
        }

        // Skip leading empty lines
        if result.is_empty() && is_empty {
            continue;
        }

        result.push(line);
        prev_empty = is_empty;
    }

    // Remove trailing empty lines
    while result.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        result.pop();
    }

    result.join("\n")
}

/// Check if text contains reasoning blocks.
pub fn has_reasoning(text: &str) -> bool {
    THINKING_TAG_REGEX.is_match(text) || has_thinking_section(text)
}

/// Check if text has a thinking section header.
fn has_thinking_section(text: &str) -> bool {
    let thinking_headers = [
        "## Thinking", "## Reasoning", "## Thought", "## Ragionamento",
    ];
    thinking_headers.iter().any(|h| text.contains(h))
}

/// Extract reasoning blocks from text (for debugging).
/// Returns a list of reasoning block contents found.
pub fn extract_reasoning(_text: &str) -> Vec<String> {
    // Simplified implementation - just return empty for now
    // Can be expanded later if needed
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_think_tags() {
        let tag = "think";
        let input = format!("Let me think.\n<{tag}>Reasoning here</{tag}>Actual answer.");
        let result = strip_reasoning(&input);
        assert!(!result.contains("<think"));
        assert!(result.contains("Actual answer"));
    }

    #[test]
    fn test_strip_thinking_section() {
        let input = "## Thinking\nMy reasoning\n\n## Answer\nThe real answer.";
        let result = strip_reasoning(input);
        assert!(!result.contains("## Thinking"));
        assert!(result.contains("## Answer"));
    }

    #[test]
    fn test_no_reasoning() {
        let input = "Hello! This is a normal response.";
        let result = strip_reasoning(input);
        assert_eq!(result.trim(), input);
    }

    #[test]
    fn test_clean_whitespace() {
        let input = "Line 1\n\n\n\nLine 2\n\n\n";
        let result = clean_whitespace(input);
        assert_eq!(result, "Line 1\n\nLine 2");
    }

    #[test]
    fn test_has_reasoning_tag() {
        let tag = "think";
        let with_tag = format!("<{tag}>Test</{tag}>");
        assert!(has_reasoning(&with_tag));
        assert!(!has_reasoning("Normal text"));
    }

    #[test]
    fn test_has_reasoning_section() {
        assert!(has_reasoning("## Thinking\nSome thoughts"));
        assert!(!has_reasoning("## Normal Section"));
    }

    #[test]
    fn test_italian_thinking() {
        let input = "## Ragionamento\nPensando...\n\n## Risposta\nEcco la risposta.";
        let result = strip_reasoning(input);
        assert!(!result.contains("Ragionamento"));
        assert!(result.contains("Risposta"));
    }
}
