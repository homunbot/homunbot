use std::collections::HashSet;

use axum::extract::Query;
use axum::response::Json;
use serde::Deserialize;

use super::helpers::{
    annotate_query_items, apply_market_entry, apply_query_recommendation, display_name_from_package,
    fetch_official_registry_servers, load_mcpmarket_index, looks_like_mcp_server_package,
    official_registry_entry_to_view, preset_to_view, sort_mcp_catalog, sort_mcp_catalog_for_query,
};
use super::{McpCatalogItemView, NpmSearchResponse};

// ── List full catalog ────────────────────────────────────────────

pub(super) async fn list_mcp_catalog() -> Json<Vec<McpCatalogItemView>> {
    let leaderboard = load_mcpmarket_index(100).await;
    let mut items = crate::skills::all_mcp_presets()
        .into_iter()
        .map(preset_to_view)
        .collect::<Vec<_>>();
    for item in &mut items {
        apply_market_entry(item, &leaderboard);
    }

    let mut seen = items
        .iter()
        .map(|i| i.id.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let official = fetch_official_registry_servers(None, 100).await;
    for entry in official {
        let Some(mut item) = official_registry_entry_to_view(entry) else {
            continue;
        };
        let dedupe_key = item.id.to_ascii_lowercase();
        if !seen.insert(dedupe_key) {
            continue;
        }
        apply_market_entry(&mut item, &leaderboard);
        items.push(item);
    }

    sort_mcp_catalog(&mut items);
    Json(items)
}

// ── Suggest (quick preset match) ─────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct McpSuggestQuery {
    q: String,
}

pub(super) async fn suggest_mcp_catalog(
    Query(query): Query<McpSuggestQuery>,
) -> Json<Vec<McpCatalogItemView>> {
    let leaderboard = load_mcpmarket_index(100).await;
    let mut items = crate::skills::suggest_mcp_presets(&query.q)
        .into_iter()
        .map(preset_to_view)
        .collect::<Vec<_>>();
    for item in &mut items {
        apply_market_entry(item, &leaderboard);
    }
    sort_mcp_catalog(&mut items);
    Json(items)
}

// ── Full search (presets + official registry + npm) ──────────────

#[derive(Deserialize)]
pub(crate) struct McpSearchQuery {
    q: String,
    limit: Option<usize>,
}

pub(super) async fn search_mcp_catalog(
    Query(query): Query<McpSearchQuery>,
) -> Json<Vec<McpCatalogItemView>> {
    let q = query.q.trim();
    let limit = query.limit.unwrap_or(20).clamp(1, 50);
    let leaderboard = load_mcpmarket_index(100).await;

    if q.is_empty() {
        return list_mcp_catalog().await;
    }

    let mut out: Vec<McpCatalogItemView> = crate::skills::suggest_mcp_presets(q)
        .into_iter()
        .map(preset_to_view)
        .collect();
    for item in &mut out {
        apply_market_entry(item, &leaderboard);
    }

    let mut seen = out
        .iter()
        .map(|r| r.id.to_lowercase())
        .collect::<HashSet<_>>();

    let official_results =
        fetch_official_registry_servers(Some(q), (limit * 4).clamp(20, 100)).await;
    for entry in official_results {
        let Some(mut item) = official_registry_entry_to_view(entry) else {
            continue;
        };
        let dedupe_key = item.id.to_ascii_lowercase();
        if !seen.insert(dedupe_key) {
            continue;
        }
        apply_market_entry(&mut item, &leaderboard);
        out.push(item);
        if out.len() >= limit {
            break;
        }
    }

    let mut url = match reqwest::Url::parse("https://registry.npmjs.org/-/v1/search") {
        Ok(u) => u,
        Err(_) => return Json(out.into_iter().take(limit).collect()),
    };
    url.query_pairs_mut()
        .append_pair("text", &format!("{} mcp", q))
        .append_pair("size", &(limit * 4).to_string());

    let client = reqwest::Client::builder()
        .user_agent("homun")
        .timeout(std::time::Duration::from_secs(10))
        .build();

    if let Ok(client) = client {
        match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(parsed) = resp.json::<NpmSearchResponse>().await {
                    for entry in parsed.objects {
                        let description = entry.package.description.unwrap_or_default();
                        if !looks_like_mcp_server_package(&entry.package.name, &description) {
                            continue;
                        }
                        let id = entry.package.name.clone();
                        if seen.contains(&id.to_lowercase()) {
                            continue;
                        }
                        let docs_url = entry.package.links.as_ref().and_then(|l| {
                            l.npm
                                .clone()
                                .or(l.repository.clone())
                                .or(l.homepage.clone())
                        });
                        let mut item = McpCatalogItemView {
                            kind: "npm".to_string(),
                            source: "npm".to_string(),
                            id: id.clone(),
                            display_name: display_name_from_package(&id),
                            description,
                            command: "npx".to_string(),
                            args: vec!["-y".to_string(), id.clone()],
                            transport: Some("stdio".to_string()),
                            url: None,
                            install_supported: true,
                            package_name: Some(id.clone()),
                            downloads_monthly: entry.downloads.and_then(|d| d.monthly),
                            score: Some(entry.score.final_score),
                            popularity_rank: None,
                            popularity_value: None,
                            popularity_source: None,
                            env: vec![],
                            docs_url,
                            aliases: vec![],
                            keywords: vec![],
                            recommended: false,
                            recommended_reason: None,
                            decision_tags: vec![],
                            setup_effort: "Moderate".to_string(),
                            auth_profile: "Unknown".to_string(),
                            preflight_checks: vec![],
                            why_choose: None,
                            tradeoff: None,
                        };
                        apply_market_entry(&mut item, &leaderboard);
                        out.push(item);
                        seen.insert(id.to_lowercase());
                        if out.len() >= limit {
                            break;
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, query = %q, "MCP npm search failed");
            }
        }
    }

    annotate_query_items(&mut out, q);
    sort_mcp_catalog_for_query(&mut out, q);
    apply_query_recommendation(&mut out, q);
    Json(out.into_iter().take(limit).collect())
}
