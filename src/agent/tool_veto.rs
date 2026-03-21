//! Tool call veto system — runtime policy enforcement.
//!
//! Prevents the LLM from calling tools in suboptimal order
//! (e.g. browser before web_search, weather for sports queries).
//! Pure function — no state, no side effects.

use std::collections::HashSet;

use super::browser_task_plan::BrowserRoutingDecision;

/// Check if a tool call should be vetoed based on runtime policies.
///
/// Returns `Some(reason)` if the call should be blocked, `None` to allow it.
/// Policies enforce:
/// - Weather tool only for actual forecasts (not sports/news)
/// - Search-first: web_search before web_fetch
/// - Browser only when web_search is insufficient
/// - Shell not for web lookups when web tools available
pub(crate) fn veto_tool_call(
    tool_name: &str,
    user_prompt: &str,
    available_tool_names: &HashSet<String>,
    browser_routing: &BrowserRoutingDecision,
    tools_already_used: &[String],
) -> Option<String> {
    let text = user_prompt.to_ascii_lowercase();
    let has_web_search = available_tool_names.contains("web_search");
    let has_web_fetch = available_tool_names.contains("web_fetch");
    let has_browser = crate::browser::has_browser_tools(available_tool_names);
    let has_web_stack = has_web_search || has_web_fetch || has_browser;
    let explicit_weather_intent = [
        "meteo",
        "tempo",
        "weather",
        "forecast",
        "temperatura",
        "temperature",
        "pioggia",
        "rain",
        "vento",
        "wind",
        "umid",
        "sole",
        "nuvol",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    if explicit_weather_intent && tool_name == "weather" {
        return None;
    }

    let mentions_sport = [
        "partita",
        "gioca",
        "giocher",
        "match",
        "fixture",
        "schedule",
        "calendario",
        "classifica",
        "serie a",
        "champions",
        "napoli",
        "milan",
        "inter",
        "juventus",
        "torino",
        "roma",
        "lazio",
        "atalanta",
        "sport",
        "goal",
        "risultato",
        "score",
        "standing",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    let mentions_news_or_events = ["news", "notizie", "evento", "eventi"]
        .iter()
        .any(|needle| text.contains(needle));

    if tool_name == "weather" && (mentions_sport || mentions_news_or_events) {
        return Some(
            "Tool vetoed: the weather tool is only for forecasts and conditions. \
For sports schedules, fixtures, standings, events, or current news, use web_search first and then web_fetch/browser if needed."
                .to_string(),
        );
    }

    let explicit_browser_intent = [
        "usa il browser",
        "use the browser",
        "apri il browser",
        "open the browser",
        "chiudi il browser",
        "close the browser",
        "clicca",
        "click",
        "login",
        "accedi",
        "upload",
        "form",
        "snapshot",
        "navigate",
        "naviga",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    let web_research_intent = [
        "cerca",
        "search",
        "trova",
        "find",
        "news",
        "notizie",
        "ultime",
        "latest",
        "oggi",
        "current",
        "calendario",
        "fixture",
        "schedule",
        "classifica",
        "standing",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    let has_known_url =
        text.contains("http://") || text.contains("https://") || text.contains("www.");

    if browser_routing.browser_required() {
        if crate::browser::is_browser_tool(tool_name) {
            return None;
        }

        if tool_name == "web_fetch" {
            return Some(format!(
                "Tool vetoed: this request requires interactive browser automation ({}). Do not use web_fetch as a surrogate for JS-heavy booking/comparison sites; use the browser tools first.",
                browser_routing.reason()
            ));
        }

        if tool_name == "web_search" && browser_routing.named_sources_known() {
            return Some(format!(
                "Tool vetoed: the required interactive sources are already known ({}). Open them with the browser tools instead of doing a generic web search first.",
                browser_routing.required_sources().join(", ")
            ));
        }
    }

    let web_search_already_tried = tools_already_used.iter().any(|t| t == "web_search");

    // Search-first policy: web_fetch should not be used before web_search
    // unless the user explicitly gave a URL to read.
    if tool_name == "web_fetch"
        && has_web_search
        && !web_search_already_tried
        && !has_known_url
        && !explicit_browser_intent
    {
        return Some(
            "Tool vetoed: use web_search first to find the right source, \
then use web_fetch on the most relevant result URL. \
Direct web_fetch is only appropriate when the user explicitly provides a URL."
                .to_string(),
        );
    }

    if crate::browser::is_browser_tool(tool_name)
        && has_web_search
        && web_research_intent
        && !explicit_browser_intent
        && !browser_routing.browser_required()
        && !web_search_already_tried
    {
        return Some(
            "Tool vetoed: browser should not be the first step for routine web research when web_search is available. \
Use web_search first to find candidate sources, then use web_fetch or browser only if interaction or JS rendering is actually needed."
                .to_string(),
        );
    }

    let explicit_shell_intent = [
        "shell", "bash", "terminal", "comando", "command", "script", "grep", "ls ", "pwd", "cat ",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    let web_lookup_intent = web_research_intent || has_known_url;

    if tool_name == "shell" && has_web_stack && web_lookup_intent && !explicit_shell_intent {
        return Some(
            "Tool vetoed: shell should not be used for web lookup or current-information research when web_search/web_fetch/browser are available. \
Use web_search first, then web_fetch or browser if needed."
                .to_string(),
        );
    }

    None
}
