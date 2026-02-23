/// XML Tool Dispatcher — fallback for LLMs without native function calling.
/// Aligned with ZeroClaw's format for maximum compatibility.
use super::traits::{ToolCallRequest, ToolDefinition};

/// Parsed LLM response with optional thinking content
#[derive(Debug, Clone, Default)]
pub struct ParsedResponse {
    /// Clean text content (without thinking blocks)
    pub content: String,
    /// Thinking content extracted from <think&gt; tags
    pub thinking: Option<String>,
    /// Tool calls parsed from the response
    pub tool_calls: Vec<ToolCallRequest>,
}

pub fn build_tools_prompt(tools: &[ToolDefinition]) -> String {
    if tools.is_empty() { return String::new(); }

    let mut p = String::from("\n\n## Tool Use Protocol\n\n");
    p.push_str("To use a tool, wrap a JSON object in <tool_call_call> tags:\n\n");
    p.push_str("```\n<tool_call_call>\n{\"name\": \"tool_name\", \"arguments\": {\"param\": \"value\"}}\n</tool_call_call>\n```\n\n");
    p.push_str("### Available Tools\n\n");

    for t in tools {
        p.push_str(&format!("**{}**: {}\n", &t.function.name, escape_xml(&t.function.description)));
        if let Some(props) = t.function.parameters.get("properties").and_then(|v| v.as_object()) {
            let req: Vec<_> = t.function.parameters.get("required")
                .and_then(|r| r.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            p.push_str("  Parameters:\n");
            for (n, s) in props {
                let d = s.get("description").and_then(|d| d.as_str()).unwrap_or("");
                let m = if req.contains(&n.as_str()) { " (required)" } else { "" };
                p.push_str(&format!("    - {}{}: {}\n", n, m, escape_xml(d)));
            }
            p.push_str("\n");
        }
    }

    p.push_str("\n## Examples\n\n");
    p.push_str("To save user info:\n```\n<tool_call_call>\n{\"name\": \"remember\", \"arguments\": {\"key\": \"hobby\", \"value\": \"cucinare\"}}\n</tool_call_call>\n```\n\n");
    p.push_str("CRITICAL: Saying \"Fatto\", \"Salvato\", or \"Done\" does NOTHING. You MUST output the <tool_call_call> tag.\n");

    p
}

pub fn parse_tool_calls(text: &str) -> (String, Vec<ToolCallRequest>) {
    let parsed = parse_response(text);
    (parsed.content, parsed.tool_calls)
}

/// Parse a complete LLM response, extracting thinking blocks, tool calls, and clean content.
/// Supports DeepSeek R1-style <think&gt; tags and various tool call formats.
pub fn parse_response(text: &str) -> ParsedResponse {
    let mut result = ParsedResponse::default();
    let mut rem = text;
    let mut tool_cnt = 0u32;

    loop {
        // Find the next special block (thinking or tool call)
        let think_pos = find_tag(rem, "<think&gt;");
        let tool_info = find_tool_start(rem);
        
        // Determine which comes first
        let next = match (think_pos, tool_info) {
            (Some(tp), Some((_, _, tool_pos))) if tp < tool_pos => "think",
            (Some(_), None) => "think",
            (Some(_), Some((_, _, tool_pos))) if think_pos.unwrap_or(usize::MAX) > tool_pos => "tool",
            (None, Some(_)) => "tool",
            (None, None) => {
                // No more special blocks, add remaining text
                if !rem.trim().is_empty() {
                    result.content.push_str(rem.trim());
                }
                break;
            }
            _ => {
                if !rem.trim().is_empty() {
                    result.content.push_str(rem.trim());
                }
                break;
            }
        };

        if next == "think" {
            let pos = think_pos.unwrap();
            // Add text before thinking block
            if pos > 0 {
                let before = rem[..pos].trim();
                if !before.is_empty() {
                    if !result.content.is_empty() {
                        result.content.push(' ');
                    }
                    result.content.push_str(before);
                }
            }
            
            // Extract thinking content
            let after_open = &rem[pos + 7..]; // Skip "<think&gt;"
            if let Some(end_pos) = find_tag(after_open, "</think&gt;") {
                let thinking = after_open[..end_pos].trim();
                if !thinking.is_empty() {
                    if let Some(ref mut t) = result.thinking {
                        t.push('\n');
                        t.push_str(thinking);
                    } else {
                        result.thinking = Some(thinking.to_string());
                    }
                }
                rem = &after_open[end_pos + 8..]; // Skip "</think&gt;"
            } else {
                // No closing tag, take rest as thinking
                let thinking = after_open.trim();
                if !thinking.is_empty() {
                    result.thinking = Some(thinking.to_string());
                }
                break;
            }
        } else {
            let (start, end, pos) = tool_info.unwrap();
            // Add text before tool call
            if pos > 0 {
                let before = rem[..pos].trim();
                if !before.is_empty() {
                    if !result.content.is_empty() {
                        result.content.push(' ');
                    }
                    result.content.push_str(before);
                }
            }
            
            let after = pos + start.len();
            if let Some(e) = rem[after..].find(end) {
                let json = rem[after..after+e].trim();
                if let Some(tc) = parse_json(json, &mut tool_cnt) {
                    result.tool_calls.push(tc);
                }
                rem = &rem[after+e+end.len()..];
            } else {
                break;
            }
        }
    }

    result.content = result.content.trim().to_string();
    result
}

fn find_tag(text: &str, tag: &str) -> Option<usize> {
    text.find(tag)
}

fn find_tool_start(t: &str) -> Option<(&'static str, &'static str, usize)> {
    let patterns = [
        ("<tool_call_call>", "</tool_call_call>"),
        ("<tool_call_call\n", "</tool_call_call>"),
        ("[TOOL_CALL]", "[/TOOL_CALL]"),
        (".tool_call\n", ".end_tool_call"),
        (".tool_call", ".end_tool_call"),
    ];
    let mut best: Option<(&str, &str, usize)> = None;
    for (s, e) in patterns {
        if let Some(i) = t.find(s) {
            if best.is_none() || i < best.unwrap().2 {
                best = Some((s, e, i));
            }
        }
    }
    best
}

fn parse_json(s: &str, cnt: &mut u32) -> Option<ToolCallRequest> {
    let s = if serde_json::from_str::<serde_json::Value>(s).is_err() {
        super::openai_compat::repair_json_public(s)
    } else { s.to_string() };
    let obj: serde_json::Value = serde_json::from_str(&s).ok()?;
    let name = obj.get("name")?.as_str()?.to_string();
    let args = obj.get("arguments").or_else(|| obj.get("parameters"))
        .cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    *cnt += 1;
    Some(ToolCallRequest { id: format!("xml_{}", cnt), name, arguments: args })
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse() {
        let r = "Let me save.\n<tool_call_call>\n{\"name\": \"remember\", \"arguments\": {\"key\": \"hobby\", \"value\": \"cucinare\"}}\n</tool_call_call>\nDone!";
        let (t, c) = parse_tool_calls(r);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "remember");
        assert!(t.contains("Let me save"));
    }
    #[test]
    fn test_empty() {
        let (t, c) = parse_tool_calls("No tools here");
        assert!(c.is_empty());
        assert_eq!(t, "No tools here");
    }
}
