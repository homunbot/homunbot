use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::{
    McpCatalogEnvView, McpCatalogItemView, McpLeaderboardEntry, McpMarketFallback, McpMarketItem,
    McpServerEnvView, OfficialRegistryInput, OfficialRegistryPackage, OfficialRegistryResponse,
    OfficialRegistryServerEntry,
};

// ── Preset conversion ────────────────────────────────────────────

pub(crate) fn preset_to_view(preset: crate::skills::McpServerPreset) -> McpCatalogItemView {
    McpCatalogItemView {
        kind: "preset".to_string(),
        source: "curated".to_string(),
        id: preset.id,
        display_name: preset.display_name,
        description: preset.description,
        command: preset.command,
        args: preset
            .args
            .iter()
            .map(|arg| crate::mcp_setup::render_mcp_arg_template(arg))
            .collect(),
        transport: Some("stdio".to_string()),
        url: None,
        install_supported: true,
        package_name: None,
        downloads_monthly: None,
        score: None,
        popularity_rank: None,
        popularity_value: None,
        popularity_source: None,
        env: preset
            .env
            .into_iter()
            .map(|e| McpCatalogEnvView {
                key: e.key,
                description: e.description,
                required: e.required,
                secret: e.secret,
            })
            .collect(),
        docs_url: preset.docs_url,
        aliases: preset.aliases,
        keywords: preset.keywords,
        recommended: false,
        recommended_reason: None,
        decision_tags: vec![],
        setup_effort: "Moderate".to_string(),
        auth_profile: "Unknown".to_string(),
        preflight_checks: vec![],
        why_choose: None,
        tradeoff: None,
    }
}

// ── Lookup key normalisation ─────────────────────────────────────

pub(crate) fn normalize_mcp_lookup_key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub(crate) fn trim_numeric_suffix(value: &str) -> &str {
    if let Some((base, suffix)) = value.rsplit_once('-') {
        if suffix.chars().all(|c| c.is_ascii_digit()) {
            return base;
        }
    }
    value
}

// ── Market index (leaderboard) ───────────────────────────────────

fn add_market_lookup_keys(
    map: &mut HashMap<String, McpLeaderboardEntry>,
    item: &McpMarketItem,
    key: &str,
) {
    let normalized = normalize_mcp_lookup_key(key);
    if normalized.is_empty() {
        return;
    }
    let incoming = McpLeaderboardEntry {
        rank: item.rank,
        popularity: item.popularity,
        url: item.url.clone(),
    };
    match map.get(&normalized) {
        Some(existing) if existing.rank <= incoming.rank => {}
        _ => {
            map.insert(normalized, incoming);
        }
    }
}

pub(crate) fn build_market_index(items: &[McpMarketItem]) -> HashMap<String, McpLeaderboardEntry> {
    let mut out = HashMap::new();
    for item in items {
        add_market_lookup_keys(&mut out, item, &item.name);
        add_market_lookup_keys(&mut out, item, &item.slug);
        add_market_lookup_keys(&mut out, item, trim_numeric_suffix(&item.slug));
    }
    out
}

pub(crate) fn find_market_entry(
    leaderboard: &HashMap<String, McpLeaderboardEntry>,
    display_name: &str,
    id: &str,
    package_name: Option<&str>,
) -> Option<McpLeaderboardEntry> {
    let mut keys = Vec::new();
    keys.push(display_name.to_string());
    keys.push(id.to_string());
    if let Some((_, tail)) = id.rsplit_once('/') {
        keys.push(tail.to_string());
    }
    if let Some((_, tail)) = id.rsplit_once('.') {
        keys.push(tail.to_string());
    }
    if let Some(pkg) = package_name {
        keys.push(pkg.to_string());
        if let Some((_, tail)) = pkg.rsplit_once('/') {
            keys.push(tail.to_string());
        }
    }

    for key in keys {
        let normalized = normalize_mcp_lookup_key(&key);
        if normalized.is_empty() {
            continue;
        }
        if let Some(entry) = leaderboard.get(&normalized) {
            return Some(entry.clone());
        }
    }
    None
}

pub(crate) fn apply_market_entry(
    item: &mut McpCatalogItemView,
    leaderboard: &HashMap<String, McpLeaderboardEntry>,
) {
    if let Some(entry) = find_market_entry(
        leaderboard,
        &item.display_name,
        &item.id,
        item.package_name.as_deref(),
    ) {
        item.popularity_rank = Some(entry.rank);
        item.popularity_value = Some(entry.popularity);
        item.popularity_source = Some("mcpmarket".to_string());
        if item.docs_url.is_none() {
            item.docs_url = Some(entry.url);
        }
    }
}

// ── Sorting ──────────────────────────────────────────────────────

pub(crate) fn sort_mcp_catalog(items: &mut [McpCatalogItemView]) {
    items.sort_by(|a, b| {
        let a_rank = a.popularity_rank.unwrap_or(u32::MAX);
        let b_rank = b.popularity_rank.unwrap_or(u32::MAX);
        let a_kind = if a.kind == "preset" { 0u8 } else { 1u8 };
        let b_kind = if b.kind == "preset" { 0u8 } else { 1u8 };
        a_rank
            .cmp(&b_rank)
            .then_with(|| a_kind.cmp(&b_kind))
            .then_with(|| a.display_name.cmp(&b.display_name))
    });
}

pub(crate) fn sort_mcp_catalog_for_query(items: &mut [McpCatalogItemView], query: &str) {
    items.sort_by(|a, b| {
        recommendation_score(b, query)
            .cmp(&recommendation_score(a, query))
            .then_with(|| {
                let a_rank = a.popularity_rank.unwrap_or(u32::MAX);
                let b_rank = b.popularity_rank.unwrap_or(u32::MAX);
                a_rank.cmp(&b_rank)
            })
            .then_with(|| a.display_name.cmp(&b.display_name))
    });
}

// ── Query text helpers ───────────────────────────────────────────

pub(crate) fn mcp_query_terms(query: &str) -> Vec<String> {
    query
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| part.len() >= 2)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
}

pub(crate) fn mcp_searchable_text(item: &McpCatalogItemView) -> String {
    format!(
        "{} {} {} {} {} {}",
        item.id,
        item.display_name,
        item.description,
        item.aliases.join(" "),
        item.keywords.join(" "),
        item.docs_url.clone().unwrap_or_default()
    )
    .to_ascii_lowercase()
}

// ── Auth/effort classification ───────────────────────────────────

pub(crate) fn required_env_count(item: &McpCatalogItemView) -> usize {
    item.env.iter().filter(|env| env.required).count()
}

pub(crate) fn mcp_supports_oauth(item: &McpCatalogItemView) -> bool {
    let text = mcp_searchable_text(item);
    item.env.iter().any(|env| {
        let key = env.key.to_ascii_lowercase();
        key.contains("client_id")
            || key.contains("client_secret")
            || key.contains("refresh_token")
            || key.contains("access_token")
            || key.contains("oauth")
    }) || text.contains("oauth")
}

pub(crate) fn mcp_requires_remote_auth(item: &McpCatalogItemView) -> bool {
    item.transport.as_deref() == Some("http")
        && item.env.iter().any(|env| {
            let key = env.key.to_ascii_lowercase();
            key.contains("authorization")
                || key.contains("header")
                || key.contains("token")
                || key.contains("api_key")
        })
}

pub(crate) fn mcp_requires_token(item: &McpCatalogItemView) -> bool {
    item.env.iter().any(|env| {
        let key = env.key.to_ascii_lowercase();
        key.contains("token") || key.contains("api_key") || key.contains("secret")
    })
}

pub(crate) fn auth_profile(item: &McpCatalogItemView) -> &'static str {
    if mcp_supports_oauth(item) {
        "OAuth"
    } else if mcp_requires_remote_auth(item) {
        "Remote auth"
    } else if mcp_requires_token(item) {
        "API key / token"
    } else if item.env.is_empty() {
        "No credentials"
    } else {
        "Manual configuration"
    }
}

pub(crate) fn setup_effort_label(item: &McpCatalogItemView) -> &'static str {
    let required_env = required_env_count(item);
    if mcp_supports_oauth(item) || required_env >= 4 {
        "Advanced"
    } else if mcp_requires_remote_auth(item)
        || mcp_requires_token(item)
        || required_env >= 2
        || item.transport.as_deref() == Some("http")
    {
        "Moderate"
    } else {
        "Easy"
    }
}

pub(crate) fn preflight_checks(item: &McpCatalogItemView, query: &str) -> Vec<String> {
    let mut checks = Vec::new();
    let query_trimmed = query.trim();
    if !query_trimmed.is_empty() {
        checks.push(format!(
            "Confirm this server really matches your intent: {}.",
            query_trimmed
        ));
    }
    if mcp_supports_oauth(item) {
        checks.push(
            "Have access to the provider developer console to create an OAuth app/client."
                .to_string(),
        );
        checks.push(
            "Be ready to configure redirect/consent settings and approve the required scopes."
                .to_string(),
        );
    } else if mcp_requires_token(item) {
        checks.push(
            "Make sure you can generate an API key or access token in the provider dashboard."
                .to_string(),
        );
    }
    if item.transport.as_deref() == Some("http") {
        checks.push("Verify the remote MCP endpoint is already live and that you know the required headers.".to_string());
    } else if !item.command.trim().is_empty() {
        checks.push(format!(
            "Local runtime will execute: {} {}.",
            item.command,
            item.args.join(" ")
        ));
    }
    if required_env_count(item) > 0 {
        checks.push(format!(
            "Prepare {} required environment value(s) before starting the wizard.",
            required_env_count(item)
        ));
    }
    if item.docs_url.is_some() {
        checks.push("Keep the linked documentation open while filling credentials.".to_string());
    }
    checks.truncate(4);
    checks
}

// ── Decision / recommendation tags ───────────────────────────────

pub(crate) fn decision_tags(item: &McpCatalogItemView) -> Vec<String> {
    let mut tags = Vec::new();
    match item.source.as_str() {
        "curated" => tags.push("Curated".to_string()),
        "official-registry" => tags.push("Official".to_string()),
        _ => {}
    }
    if let Some(rank) = item.popularity_rank {
        if rank <= 20 {
            tags.push("Popular".to_string());
        }
    }
    match setup_effort_label(item) {
        "Easy" => tags.push("Easiest setup".to_string()),
        "Advanced" => tags.push("Advanced".to_string()),
        _ => {}
    }
    match auth_profile(item) {
        "OAuth" => tags.push("Requires OAuth".to_string()),
        "Remote auth" => tags.push("Remote endpoint".to_string()),
        "API key / token" => tags.push("Needs token".to_string()),
        _ => {}
    }
    tags.truncate(4);
    tags
}

pub(crate) fn why_choose_reason(item: &McpCatalogItemView) -> String {
    if item.source == "curated" {
        "Choose this if you want the cleanest guided setup inside Homun.".to_string()
    } else if item.source == "official-registry" {
        "Choose this if you prefer an MCP listed in the official registry.".to_string()
    } else if item.transport.as_deref() == Some("http") {
        "Choose this if you prefer a hosted endpoint instead of installing a local runtime."
            .to_string()
    } else if item.popularity_rank.unwrap_or(u32::MAX) <= 25 {
        "Choose this if you want a widely used option with stronger community validation."
            .to_string()
    } else {
        "Choose this only if its features match your use case better than the recommended option."
            .to_string()
    }
}

pub(crate) fn tradeoff_reason(item: &McpCatalogItemView) -> String {
    if mcp_supports_oauth(item) {
        "Tradeoff: setup is heavier because OAuth credentials and consent flow are required."
            .to_string()
    } else if item.transport.as_deref() == Some("http") {
        "Tradeoff: depends on a remote endpoint and usually on custom authorization headers."
            .to_string()
    } else if required_env_count(item) >= 3 {
        "Tradeoff: you need several environment values before the connection can work.".to_string()
    } else if item.source == "npm" {
        "Tradeoff: package is less curated, so documentation and defaults may be rougher."
            .to_string()
    } else {
        "Tradeoff: not the simplest default starting point for a non-technical user.".to_string()
    }
}

pub(crate) fn annotate_query_items(items: &mut [McpCatalogItemView], query: &str) {
    for item in items.iter_mut() {
        item.setup_effort = setup_effort_label(item).to_string();
        item.auth_profile = auth_profile(item).to_string();
        item.preflight_checks = preflight_checks(item, query);
        item.decision_tags = decision_tags(item);
        item.why_choose = Some(why_choose_reason(item));
        item.tradeoff = Some(tradeoff_reason(item));
    }
}

// ── Recommendation scoring ───────────────────────────────────────

pub(crate) fn recommendation_score(item: &McpCatalogItemView, query: &str) -> i64 {
    let query_lower = query.trim().to_ascii_lowercase();
    let terms = mcp_query_terms(query);
    let searchable = mcp_searchable_text(item);
    let name_text = format!("{} {}", item.display_name, item.id).to_ascii_lowercase();
    let mut score = 0i64;

    if !query_lower.is_empty() && searchable.contains(&query_lower) {
        score += 80;
    }
    if !query_lower.is_empty() && name_text.contains(&query_lower) {
        score += 70;
    }
    for term in terms {
        if name_text.contains(&term) {
            score += 24;
        } else if searchable.contains(&term) {
            score += 10;
        }
    }

    score += match item.source.as_str() {
        "curated" => 42,
        "official-registry" => 34,
        "npm" => 8,
        _ => 12,
    };
    score += if item.install_supported { 18 } else { -20 };
    score += if item.docs_url.is_some() { 12 } else { 0 };
    score += match item.transport.as_deref() {
        Some("stdio") => 8,
        Some("http") => 4,
        _ => 0,
    };
    score += (22usize.saturating_sub(required_env_count(item) * 4)) as i64;
    if item.env.len() > 5 {
        score -= ((item.env.len() - 5) as i64) * 2;
    }
    if let Some(rank) = item.popularity_rank {
        score += 120i64.saturating_sub(rank.min(120) as i64);
    }
    if item.package_name.is_some() {
        score += 4;
    }

    score
}

pub(crate) fn recommendation_reason(item: &McpCatalogItemView) -> String {
    let mut reasons = Vec::new();
    match item.source.as_str() {
        "curated" => reasons.push("curated by Homun for guided setup".to_string()),
        "official-registry" => reasons.push("listed in the official MCP registry".to_string()),
        _ => {}
    }
    if item.install_supported {
        reasons.push("works with the guided installer".to_string());
    }
    if let Some(rank) = item.popularity_rank {
        reasons.push(format!("ranked #{} in the MCPMarket Top 100", rank));
    }
    let required_env = required_env_count(item);
    if required_env <= 2 {
        reasons.push("requires only a small number of credentials".to_string());
    } else if required_env <= 4 {
        reasons.push("setup stays reasonably compact".to_string());
    }
    if item.docs_url.is_some() {
        reasons.push("documentation is linked for credential lookup".to_string());
    }

    if reasons.is_empty() {
        "best overall match for the requested service".to_string()
    } else {
        reasons.truncate(3);
        reasons.join(", ")
    }
}

pub(crate) fn apply_query_recommendation(items: &mut [McpCatalogItemView], query: &str) {
    for item in items.iter_mut() {
        item.recommended = false;
        item.recommended_reason = None;
    }
    if query.trim().is_empty() || items.is_empty() {
        return;
    }

    let best_index = items
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            recommendation_score(a, query)
                .cmp(&recommendation_score(b, query))
                .then_with(|| {
                    let a_rank = a.popularity_rank.unwrap_or(u32::MAX);
                    let b_rank = b.popularity_rank.unwrap_or(u32::MAX);
                    b_rank.cmp(&a_rank)
                })
        })
        .map(|(idx, _)| idx);

    if let Some(idx) = best_index {
        items[idx].recommended = true;
        items[idx].recommended_reason = Some(recommendation_reason(&items[idx]));
        if !items[idx]
            .decision_tags
            .iter()
            .any(|tag| tag == "Recommended")
        {
            items[idx]
                .decision_tags
                .insert(0, "Recommended".to_string());
        }
    }
}

// ── MCPMarket fetching ───────────────────────────────────────────

pub(crate) fn parse_market_item_list(
    value: &serde_json::Value,
    limit: usize,
) -> Vec<McpMarketItem> {
    let Some(item_list) = value
        .get("@type")
        .and_then(|v| v.as_str())
        .filter(|t| *t == "ItemList")
        .and_then(|_| value.get("itemListElement"))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };

    item_list
        .iter()
        .filter_map(|entry| {
            let rank = entry
                .get("position")
                .and_then(|v| v.as_u64())
                .and_then(|n| u32::try_from(n).ok())?;
            let item = entry.get("item")?;
            let name = item.get("name").and_then(|v| v.as_str())?.to_string();
            let url = item.get("url").and_then(|v| v.as_str())?.to_string();
            let slug = url
                .split("/server/")
                .nth(1)
                .unwrap_or_default()
                .split('?')
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            if slug.is_empty() {
                return None;
            }
            let popularity = item
                .get("interactionStatistic")
                .and_then(|v| v.get("userInteractionCount"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            Some(McpMarketItem {
                rank,
                name,
                slug,
                popularity,
                url,
            })
        })
        .take(limit)
        .collect()
}

pub(crate) async fn fetch_mcpmarket_live(limit: usize) -> Option<Vec<McpMarketItem>> {
    let client = reqwest::Client::builder()
        .user_agent("homun")
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    let response = client
        .get("https://mcpmarket.com/leaderboards")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let html = response.text().await.ok()?;
    let script_re = regex::Regex::new(
        r#"(?s)<script[^>]*type=["']application/ld\+json["'][^>]*>(.*?)</script>"#,
    )
    .ok()?;
    for cap in script_re.captures_iter(&html) {
        let payload = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
            let items = parse_market_item_list(&value, limit);
            if !items.is_empty() {
                return Some(items);
            }
        }
    }
    None
}

pub(crate) async fn load_mcpmarket_fallback(limit: usize) -> Vec<McpMarketItem> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("static")
        .join("data")
        .join("mcpmarket-top100-fallback.json");
    let Ok(content) = tokio::fs::read_to_string(path).await else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<McpMarketFallback>(&content) else {
        return Vec::new();
    };
    parsed
        .items
        .into_iter()
        .map(|i| McpMarketItem {
            rank: i.rank,
            name: i.name,
            slug: i.slug.clone(),
            popularity: i.popularity,
            url: i
                .url
                .unwrap_or_else(|| format!("https://mcpmarket.com/server/{}", i.slug)),
        })
        .take(limit)
        .collect()
}

#[allow(clippy::type_complexity)]
pub(crate) async fn load_mcpmarket_index(limit: usize) -> HashMap<String, McpLeaderboardEntry> {
    static MCPMARKET_INDEX_CACHE: OnceLock<
        Mutex<Option<(Instant, HashMap<String, McpLeaderboardEntry>)>>,
    > = OnceLock::new();
    const CACHE_TTL: Duration = Duration::from_secs(15 * 60);

    let cache = MCPMARKET_INDEX_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((cached_at, data)) = guard.as_ref() {
            if cached_at.elapsed() < CACHE_TTL {
                return data.clone();
            }
        }
    }

    let fresh = if let Some(items) = fetch_mcpmarket_live(limit).await {
        build_market_index(&items)
    } else {
        let fallback = load_mcpmarket_fallback(limit).await;
        if fallback.is_empty() {
            tracing::warn!(
                "MCPMarket leaderboard unavailable; continuing without popularity ranking"
            );
            HashMap::new()
        } else {
            tracing::warn!(
                "Using bundled MCPMarket leaderboard fallback (live source unavailable)"
            );
            build_market_index(&fallback)
        }
    };

    if let Ok(mut guard) = cache.lock() {
        *guard = Some((Instant::now(), fresh.clone()));
    }

    fresh
}

// ── Official registry conversion ─────────────────────────────────

pub(crate) fn package_command_and_args(
    package: &OfficialRegistryPackage,
) -> Option<(String, Vec<String>)> {
    let identifier = package.identifier.as_deref()?.trim();
    if identifier.is_empty() {
        return None;
    }
    let runtime_hint = package
        .runtime_hint
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let registry_type = package
        .registry_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let version = package
        .version
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());

    if runtime_hint == "npx" || registry_type == "npm" {
        let mut package_ref = identifier.to_string();
        if let Some(ver) = version {
            package_ref = format!("{}@{}", identifier, ver);
        }
        return Some((
            "npx".to_string(),
            vec!["-y".to_string(), package_ref.to_string()],
        ));
    }

    if runtime_hint == "uvx" || registry_type == "pypi" {
        let package_ref = if let Some(ver) = version {
            format!("{}=={}", identifier, ver)
        } else {
            identifier.to_string()
        };
        return Some(("uvx".to_string(), vec![package_ref]));
    }

    if runtime_hint == "docker" || registry_type == "oci" {
        return Some((
            "docker".to_string(),
            vec![
                "run".to_string(),
                "--rm".to_string(),
                "-i".to_string(),
                identifier.to_string(),
            ],
        ));
    }

    None
}

pub(crate) fn build_env_view(specs: &[OfficialRegistryInput]) -> Vec<McpCatalogEnvView> {
    specs
        .iter()
        .filter_map(|spec| {
            let key = spec.name.clone()?.trim().to_string();
            if key.is_empty() {
                return None;
            }
            Some(McpCatalogEnvView {
                key: key.clone(),
                description: spec
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("Value for {}", key)),
                required: spec.is_required.unwrap_or(false),
                secret: spec.is_secret.unwrap_or(false),
            })
        })
        .collect()
}

pub(crate) fn official_registry_entry_to_view(
    entry: OfficialRegistryServerEntry,
) -> Option<McpCatalogItemView> {
    let server = entry.server;
    let id = server.name.trim().to_string();
    if id.is_empty() {
        return None;
    }

    let display_name = server
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| display_name_from_package(&id));

    let description = server
        .description
        .clone()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| "MCP server from official registry".to_string());

    let docs_url = server.website_url.or_else(|| {
        server
            .repository
            .as_ref()
            .and_then(|repo| repo.url.as_ref().map(ToString::to_string))
    });

    let packages = server.packages.unwrap_or_default();
    if let Some(pkg) = packages
        .iter()
        .find(|p| package_command_and_args(p).is_some())
    {
        let (command, args) = package_command_and_args(pkg)?;
        let env = build_env_view(pkg.environment_variables.as_deref().unwrap_or_default());
        return Some(McpCatalogItemView {
            kind: "registry".to_string(),
            source: "official-registry".to_string(),
            id,
            display_name,
            description,
            command,
            args,
            transport: Some("stdio".to_string()),
            url: None,
            install_supported: true,
            package_name: pkg.identifier.clone(),
            downloads_monthly: None,
            score: None,
            popularity_rank: None,
            popularity_value: None,
            popularity_source: None,
            env,
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
        });
    }

    let remotes = server.remotes.unwrap_or_default();
    if let Some(remote) = remotes.into_iter().find(|r| {
        matches!(
            r.transport_type
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "streamable-http" | "sse" | "http" | "https"
        ) && r.url.as_deref().map(str::trim).is_some()
    }) {
        let env = build_env_view(remote.headers.as_deref().unwrap_or_default());
        return Some(McpCatalogItemView {
            kind: "registry".to_string(),
            source: "official-registry".to_string(),
            id,
            display_name,
            description,
            command: String::new(),
            args: vec![],
            transport: Some("http".to_string()),
            url: remote.url,
            install_supported: true,
            package_name: None,
            downloads_monthly: None,
            score: None,
            popularity_rank: None,
            popularity_value: None,
            popularity_source: None,
            env,
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
        });
    }

    Some(McpCatalogItemView {
        kind: "registry".to_string(),
        source: "official-registry".to_string(),
        id,
        display_name,
        description,
        command: String::new(),
        args: vec![],
        transport: None,
        url: None,
        install_supported: false,
        package_name: None,
        downloads_monthly: None,
        score: None,
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
    })
}

pub(crate) async fn fetch_official_registry_servers(
    search: Option<&str>,
    limit: usize,
) -> Vec<OfficialRegistryServerEntry> {
    let mut url = match reqwest::Url::parse("https://registry.modelcontextprotocol.io/v0/servers") {
        Ok(u) => u,
        Err(_) => return Vec::new(),
    };

    let safe_limit = limit.clamp(1, 100);
    url.query_pairs_mut()
        .append_pair("version", "latest")
        .append_pair("limit", &safe_limit.to_string());
    if let Some(q) = search.map(str::trim).filter(|q| !q.is_empty()) {
        url.query_pairs_mut().append_pair("search", q);
    }

    let client = match reqwest::Client::builder()
        .user_agent("homun")
        .timeout(Duration::from_secs(12))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    match client.get(url).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<OfficialRegistryResponse>().await {
                Ok(parsed) => parsed.servers.unwrap_or_default(),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to parse official MCP registry response");
                    Vec::new()
                }
            }
        }
        Ok(resp) => {
            tracing::warn!(
                status = %resp.status(),
                "Official MCP registry request returned non-success status"
            );
            Vec::new()
        }
        Err(e) => {
            tracing::warn!(error = %e, "Official MCP registry request failed");
            Vec::new()
        }
    }
}

// ── Misc helpers ─────────────────────────────────────────────────

pub(crate) fn normalize_mcp_capabilities(values: &[String]) -> Vec<String> {
    let mut out = values
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

pub(crate) fn mcp_env_preview(value: &str) -> (String, bool) {
    if value.starts_with("vault://") {
        (value.to_string(), true)
    } else if value.is_empty() {
        (String::new(), false)
    } else {
        ("(set)".to_string(), false)
    }
}

pub(crate) fn looks_like_mcp_server_package(name: &str, description: &str) -> bool {
    let n = name.to_lowercase();
    let d = description.to_lowercase();
    n.contains("mcp")
        || d.contains("model context protocol")
        || d.contains("mcp server")
        || d.contains("model-context-protocol")
}

pub(crate) fn display_name_from_package(pkg: &str) -> String {
    let mut name = pkg.to_string();
    if let Some((_, rest)) = name.split_once('/') {
        name = rest.to_string();
    }
    name = name
        .replace("server-", "")
        .replace("-server", "")
        .replace("mcp-", "")
        .replace("-mcp", "")
        .replace(['.', '_', '-'], " ");
    name.split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
