use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::web::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/v1/automations",
            get(list_automations).post(create_automation),
        )
        .route("/v1/automations/targets", get(list_automation_targets))
        .route(
            "/v1/automations/generate-flow",
            axum::routing::post(generate_automation_flow),
        )
        .route(
            "/v1/automations/{id}",
            axum::routing::patch(patch_automation).delete(delete_automation),
        )
        .route("/v1/automations/{id}/history", get(get_automation_history))
        .route(
            "/v1/automations/{id}/run",
            axum::routing::post(run_automation_now),
        )
}

// --- Types ---

#[derive(Deserialize)]
struct CreateAutomationRequest {
    name: String,
    prompt: String,
    schedule: Option<String>,
    cron: Option<String>,
    every: Option<u64>,
    trigger: Option<String>,
    trigger_value: Option<String>,
    enabled: Option<bool>,
    deliver_to: Option<String>,
    workflow_steps: Option<Vec<serde_json::Value>>,
    flow_json: Option<String>,
}

#[derive(Deserialize)]
struct PatchAutomationRequest {
    name: Option<String>,
    prompt: Option<String>,
    schedule: Option<String>,
    cron: Option<String>,
    every: Option<u64>,
    trigger: Option<String>,
    trigger_value: Option<String>,
    clear_trigger_value: Option<bool>,
    enabled: Option<bool>,
    status: Option<String>,
    deliver_to: Option<String>,
    clear_deliver_to: Option<bool>,
    workflow_steps: Option<Vec<serde_json::Value>>,
    clear_workflow_steps: Option<bool>,
    flow_json: Option<String>,
}

#[derive(Deserialize)]
struct AutomationHistoryQuery {
    limit: Option<u32>,
}

#[derive(Serialize)]
struct RunAutomationResponse {
    run_id: String,
    status: String,
    message: String,
}

#[derive(Serialize)]
struct AutomationListItem {
    #[serde(flatten)]
    row: crate::storage::AutomationRow,
    next_run: Option<String>,
}

#[derive(Serialize)]
struct AutomationTarget {
    value: String,
    label: String,
}

#[derive(Deserialize)]
struct GenerateFlowRequest {
    description: String,
}

#[derive(Serialize)]
struct GenerateFlowResponse {
    name: String,
    flow: serde_json::Value,
}

// --- Helpers ---

fn automation_channel_label(channel: &str) -> String {
    match channel {
        "telegram" => "Telegram".to_string(),
        "discord" => "Discord".to_string(),
        "slack" => "Slack".to_string(),
        "whatsapp" => "WhatsApp".to_string(),
        "web" => "Web".to_string(),
        ch if ch.starts_with("email:") => format!("Email ({})", &ch[6..]),
        "email" => "Email".to_string(),
        other => other.to_string(),
    }
}

fn resolve_automation_target_chat_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with("vault://") {
        return trimmed.to_string();
    }

    let Some(key) = trimmed.strip_prefix("vault://") else {
        return trimmed.to_string();
    };
    let vault_key = key.trim();
    if vault_key.is_empty() {
        return trimmed.to_string();
    }

    if let Ok(secrets) = crate::storage::global_secrets() {
        let secret_key = crate::storage::SecretKey::custom(&format!("vault.{vault_key}"));
        if let Ok(Some(value)) = secrets.get(&secret_key) {
            let resolved = value.trim();
            if !resolved.is_empty() {
                return resolved.to_string();
            }
        }
    }
    trimmed.to_string()
}

fn build_automation_schedule(
    schedule: Option<&str>,
    cron: Option<&str>,
    every: Option<u64>,
) -> Result<String, (StatusCode, String)> {
    if let Some(s) = schedule {
        return crate::scheduler::AutomationSchedule::parse_stored(s)
            .map(|v| v.as_stored())
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()));
    }

    match (cron, every) {
        (Some(expr), None) => crate::scheduler::AutomationSchedule::from_cron(expr)
            .map(|v| v.as_stored())
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string())),
        (None, Some(secs)) => crate::scheduler::AutomationSchedule::from_every(secs)
            .map(|v| v.as_stored())
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string())),
        _ => Err((
            StatusCode::BAD_REQUEST,
            "Provide either `schedule` or one of (`cron`, `every`)".to_string(),
        )),
    }
}

fn parse_deliver_to(deliver_to: &str) -> Result<(String, String), (StatusCode, String)> {
    let (channel, chat_id) = deliver_to.rsplit_once(':').ok_or((
        StatusCode::BAD_REQUEST,
        "deliver_to must be in format channel:chat_id".to_string(),
    ))?;
    let channel = channel.trim();
    let chat_id = chat_id.trim();
    if channel.is_empty() || chat_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "deliver_to must be in format channel:chat_id".to_string(),
        ));
    }
    Ok((channel.to_string(), chat_id.to_string()))
}

fn normalize_automation_trigger(
    trigger: Option<&str>,
    trigger_value: Option<&str>,
) -> Result<(String, Option<String>), (StatusCode, String)> {
    let trigger = trigger
        .unwrap_or("always")
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_");

    match trigger.as_str() {
        "always" => Ok(("always".to_string(), None)),
        "on_change" | "changed" => Ok(("on_change".to_string(), None)),
        "contains" => {
            let value = trigger_value
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or((
                    StatusCode::BAD_REQUEST,
                    "trigger_value is required when trigger=contains".to_string(),
                ))?;
            Ok(("contains".to_string(), Some(value.to_string())))
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            "trigger must be one of: always, on_change, contains".to_string(),
        )),
    }
}

/// Extract the outermost JSON object from a string (used for LLM responses).
fn extract_json_object_block(input: &str) -> Option<&str> {
    let start = input.find('{')?;
    let end = input.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(&input[start..=end])
}

// --- Handlers ---

/// GET /api/v1/automations/targets
async fn list_automation_targets(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<AutomationTarget>> {
    let mut channels = {
        let cfg = state.config.read().await;
        cfg.channels.clone()
    };
    channels.migrate_legacy_email();

    let mut seen = HashSet::new();
    let mut targets = vec![AutomationTarget {
        value: "cli:default".to_string(),
        label: "CLI (default)".to_string(),
    }];
    seen.insert("cli:default".to_string());

    for (channel, chat_id) in channels.active_channels_with_chat_ids() {
        let chat_id = resolve_automation_target_chat_id(&chat_id);
        if chat_id.is_empty() || chat_id.starts_with("vault://") {
            continue;
        }
        let value = format!("{channel}:{chat_id}");
        if !seen.insert(value.clone()) {
            continue;
        }
        let label = format!("{} ({chat_id})", automation_channel_label(&channel));
        targets.push(AutomationTarget { value, label });
    }

    if let Some(db) = &state.db {
        if let Ok(users) = db.load_all_users().await {
            if let Some(owner) = users.into_iter().next() {
                if let Ok(identities) = db.load_user_identities(&owner.id).await {
                    for identity in identities {
                        let channel = identity.channel.trim().to_ascii_lowercase();
                        let platform_id = identity.platform_id.trim().to_string();
                        if channel.is_empty() || platform_id.is_empty() {
                            continue;
                        }

                        let value = format!("{channel}:{platform_id}");
                        if !seen.insert(value.clone()) {
                            continue;
                        }

                        let label_suffix = identity
                            .display_name
                            .as_deref()
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .unwrap_or(&platform_id);
                        let label =
                            format!("{} ({label_suffix})", automation_channel_label(&channel));
                        targets.push(AutomationTarget { value, label });
                    }
                }
            }
        }
    }

    if let Some(cli_idx) = targets.iter().position(|t| t.value == "cli:default") {
        let cli = targets.remove(cli_idx);
        targets.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
        targets.insert(0, cli);
    } else {
        targets.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    }

    Json(targets)
}

/// GET /api/v1/automations
async fn list_automations(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AutomationListItem>>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;
    let rows = db.load_automations().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list automations: {e}"),
        )
    })?;
    let now = chrono::Utc::now();
    let items = rows
        .into_iter()
        .map(|mut row| {
            let next_run = crate::scheduler::AutomationSchedule::next_run_from_stored(
                &row.schedule,
                row.last_run.as_deref(),
                now,
            )
            .map(|dt| dt.to_rfc3339());

            // Derive flow if not stored
            if row.flow_json.is_none() {
                let flow = crate::scheduler::derive_flow(&row);
                row.flow_json = serde_json::to_string(&flow).ok();
            }

            AutomationListItem { next_run, row }
        })
        .collect::<Vec<_>>();
    Ok(Json(items))
}

/// POST /api/v1/automations
async fn create_automation(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAutomationRequest>,
) -> Result<Json<crate::storage::AutomationRow>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;

    if req.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Name cannot be empty".to_string()));
    }
    if req.prompt.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Prompt cannot be empty".to_string(),
        ));
    }
    if let Some(deliver_to) = req.deliver_to.as_deref() {
        parse_deliver_to(deliver_to)?;
    }
    let (trigger_kind, trigger_value) =
        normalize_automation_trigger(req.trigger.as_deref(), req.trigger_value.as_deref())?;

    let schedule =
        build_automation_schedule(req.schedule.as_deref(), req.cron.as_deref(), req.every)?;
    let prompt = req.prompt.trim().to_string();
    let compiled_plan = {
        let cfg = state.config.read().await.clone();
        crate::scheduler::automations::compile_automation_plan(&prompt, &cfg)
    };
    let id = uuid::Uuid::new_v4().to_string();
    let enabled = req.enabled.unwrap_or(true);
    let status = if !enabled {
        "paused"
    } else if compiled_plan.is_valid() {
        "active"
    } else {
        "invalid_config"
    };

    let plan_json = compiled_plan.plan_json();
    let dependencies_json = compiled_plan.dependencies_json();
    let validation_errors_json = compiled_plan.validation_errors_json();
    db.insert_automation_with_plan(
        &id,
        req.name.trim(),
        &prompt,
        &schedule,
        enabled,
        status,
        req.deliver_to.as_deref(),
        &trigger_kind,
        trigger_value.as_deref(),
        Some(&plan_json),
        &dependencies_json,
        compiled_plan.plan.version,
        validation_errors_json.as_deref(),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create automation: {e}"),
        )
    })?;

    // Save workflow steps and/or flow_json if provided
    {
        let mut extra = crate::storage::AutomationUpdate::default();
        let mut has_extra = false;

        if let Some(steps) = req.workflow_steps {
            if !steps.is_empty() {
                let steps_json = serde_json::to_string(&steps).map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Invalid workflow steps: {e}"),
                    )
                })?;
                extra.workflow_steps_json = Some(Some(steps_json));
                has_extra = true;
            }
        }

        if let Some(fj) = req.flow_json {
            if !fj.is_empty() {
                extra.flow_json = Some(Some(fj));
                has_extra = true;
            }
        }

        if has_extra {
            let _ = db.update_automation(&id, extra).await;
        }
    }

    let created = db.load_automation(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load created automation: {e}"),
        )
    })?;

    created.map(Json).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Automation not found after insert".to_string(),
    ))
}

/// PATCH /api/v1/automations/{id}
async fn patch_automation(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<PatchAutomationRequest>,
) -> Result<Json<crate::storage::AutomationRow>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;

    let current = db.load_automation(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load automation: {e}"),
        )
    })?;
    let Some(current) = current else {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Automation '{id}' not found"),
        ));
    };

    let requested_status = req.status.as_deref().map(|v| v.trim().to_string());
    let mut update = crate::storage::AutomationUpdate {
        name: req.name.map(|v| v.trim().to_string()),
        prompt: req.prompt.map(|v| v.trim().to_string()),
        enabled: req.enabled,
        status: requested_status.clone(),
        ..Default::default()
    };

    if req.clear_deliver_to.unwrap_or(false) {
        update.deliver_to = Some(None);
    } else if let Some(deliver_to) = req.deliver_to {
        parse_deliver_to(&deliver_to)?;
        update.deliver_to = Some(Some(deliver_to));
    }

    if req.schedule.is_some() || req.cron.is_some() || req.every.is_some() {
        update.schedule = Some(build_automation_schedule(
            req.schedule.as_deref(),
            req.cron.as_deref(),
            req.every,
        )?);
    }

    if req.clear_trigger_value.unwrap_or(false) {
        update.trigger_value = Some(None);
        if req.trigger.is_none() && current.trigger_kind == "contains" {
            update.trigger_kind = Some("always".to_string());
        }
    }

    if req.trigger.is_some() || req.trigger_value.is_some() {
        let desired_trigger = req.trigger.as_deref().unwrap_or(&current.trigger_kind);
        let desired_trigger_value = if req.trigger_value.is_some() {
            req.trigger_value.as_deref()
        } else {
            current.trigger_value.as_deref()
        };
        let (trigger_kind, trigger_value) =
            normalize_automation_trigger(Some(desired_trigger), desired_trigger_value)?;
        update.trigger_kind = Some(trigger_kind);
        update.trigger_value = Some(trigger_value);
    }

    // Handle workflow steps
    if req.clear_workflow_steps.unwrap_or(false) {
        update.workflow_steps_json = Some(None);
    } else if let Some(steps) = req.workflow_steps {
        if steps.is_empty() {
            update.workflow_steps_json = Some(None);
        } else {
            let steps_json = serde_json::to_string(&steps).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid workflow steps: {e}"),
                )
            })?;
            update.workflow_steps_json = Some(Some(steps_json));
        }
    }

    // Handle flow_json (visual graph)
    if let Some(fj) = req.flow_json {
        if fj.is_empty() {
            update.flow_json = Some(None);
        } else {
            update.flow_json = Some(Some(fj));
        }
    }

    let final_prompt = update
        .prompt
        .clone()
        .unwrap_or_else(|| current.prompt.clone());
    let final_enabled = update.enabled.unwrap_or(current.enabled);
    let compiled_plan = {
        let cfg = state.config.read().await.clone();
        crate::scheduler::automations::compile_automation_plan(&final_prompt, &cfg)
    };
    update.plan_json = Some(Some(compiled_plan.plan_json()));
    update.dependencies_json = Some(Some(compiled_plan.dependencies_json()));
    update.plan_version = Some(compiled_plan.plan.version);
    update.validation_errors = Some(compiled_plan.validation_errors_json());

    let mut next_status = update
        .status
        .clone()
        .unwrap_or_else(|| current.status.clone());
    if final_enabled && !compiled_plan.is_valid() {
        next_status = "invalid_config".to_string();
        let summary = compiled_plan.validation_errors.join(" | ");
        update.last_result = Some(Some(format!("Automation configuration invalid: {summary}")));
    } else if !final_enabled && requested_status.is_none() {
        next_status = "paused".to_string();
    } else if final_enabled
        && compiled_plan.is_valid()
        && requested_status.is_none()
        && current.status.eq_ignore_ascii_case("invalid_config")
    {
        next_status = "active".to_string();
    }
    update.status = Some(next_status);

    let updated = db.update_automation(&id, update).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update automation: {e}"),
        )
    })?;

    if !updated {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Automation '{id}' not found (or no fields to update)"),
        ));
    }

    let row = db.load_automation(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load updated automation: {e}"),
        )
    })?;

    row.map(Json).ok_or((
        StatusCode::NOT_FOUND,
        format!("Automation '{id}' not found"),
    ))
}

/// DELETE /api/v1/automations/{id}
async fn delete_automation(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;

    let removed = db.delete_automation(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete automation: {e}"),
        )
    })?;

    if !removed {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Automation '{id}' not found"),
        ));
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "id": id
    })))
}

/// GET /api/v1/automations/{id}/history
async fn get_automation_history(
    Path(id): Path<String>,
    Query(q): Query<AutomationHistoryQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<crate::storage::AutomationRunRow>>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let runs = db.load_automation_runs(&id, limit).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load automation history: {e}"),
        )
    })?;
    Ok(Json(runs))
}

/// POST /api/v1/automations/{id}/run
async fn run_automation_now(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<RunAutomationResponse>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;

    let automation = db.load_automation(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load automation: {e}"),
        )
    })?;
    let Some(automation) = automation else {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Automation '{id}' not found"),
        ));
    };

    let compiled_plan = {
        let cfg = state.config.read().await.clone();
        crate::scheduler::automations::compile_automation_plan(&automation.prompt, &cfg)
    };
    let plan_json = compiled_plan.plan_json();
    let dependencies_json = compiled_plan.dependencies_json();
    let validation_errors_json = compiled_plan.validation_errors_json();
    let derived_status = if !automation.enabled {
        "paused".to_string()
    } else if compiled_plan.is_valid() {
        "active".to_string()
    } else {
        "invalid_config".to_string()
    };
    let _ = db
        .update_automation(
            &automation.id,
            crate::storage::AutomationUpdate {
                status: Some(derived_status.clone()),
                plan_json: Some(Some(plan_json)),
                dependencies_json: Some(Some(dependencies_json)),
                plan_version: Some(compiled_plan.plan.version),
                validation_errors: Some(validation_errors_json.clone()),
                ..Default::default()
            },
        )
        .await;

    if derived_status.eq_ignore_ascii_case("invalid_config") {
        let errors = crate::scheduler::automations::parse_validation_errors_json(
            validation_errors_json.as_deref(),
        );
        let reason = if errors.is_empty() {
            "Automation configuration is invalid. Update dependencies before running.".to_string()
        } else {
            format!(
                "Automation configuration is invalid: {}",
                errors.join(" | ")
            )
        };
        let run_id = uuid::Uuid::new_v4().to_string();
        let _ = db
            .insert_automation_run(&run_id, &automation.id, "error", Some(&reason))
            .await;
        let _ = db
            .update_automation(
                &automation.id,
                crate::storage::AutomationUpdate {
                    status: Some("invalid_config".to_string()),
                    last_result: Some(Some(reason.clone())),
                    touch_last_run: true,
                    ..Default::default()
                },
            )
            .await;
        return Ok(Json(RunAutomationResponse {
            run_id,
            status: "error".to_string(),
            message: reason,
        }));
    }

    let target = automation
        .deliver_to
        .clone()
        .unwrap_or_else(|| "cli:default".to_string());
    let (channel, chat_id) = parse_deliver_to(&target)?;

    let run_id = uuid::Uuid::new_v4().to_string();
    db.insert_automation_run(
        &run_id,
        &automation.id,
        "queued",
        Some("Manual run requested"),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create automation run: {e}"),
        )
    })?;

    let Some(inbound_tx) = &state.inbound_tx else {
        let _ = db
            .complete_automation_run(
                &run_id,
                "error",
                Some("Agent queue unavailable (setup-only mode)"),
            )
            .await;
        let _ = db
            .update_automation(
                &automation.id,
                crate::storage::AutomationUpdate {
                    status: Some("error".to_string()),
                    last_result: Some(Some(
                        "Manual run failed: agent queue unavailable".to_string(),
                    )),
                    touch_last_run: true,
                    ..Default::default()
                },
            )
            .await;
        return Ok(Json(RunAutomationResponse {
            run_id,
            status: "error".to_string(),
            message: "Agent queue unavailable (setup-only mode)".to_string(),
        }));
    };

    // Build the effective prompt: for multi-step automations, incorporate workflow steps
    let effective_prompt =
        crate::scheduler::automations::build_effective_prompt_from_row(&automation);
    let runtime_prompt = crate::scheduler::automations::build_runtime_run_input_from_plan(
        automation.plan_json.as_deref(),
        &effective_prompt,
    );

    let msg = crate::bus::InboundMessage {
        channel,
        sender_id: format!("automation:{}", automation.id),
        chat_id,
        content: runtime_prompt,
        timestamp: chrono::Utc::now(),
        metadata: Some(crate::bus::MessageMetadata {
            is_system: true,
            scheduler_kind: Some("automation".to_string()),
            scheduler_job_id: Some(automation.id.clone()),
            automation_run_id: Some(run_id.clone()),
            ..Default::default()
        }),
    };

    match inbound_tx.send(msg).await {
        Ok(()) => {
            let result_msg = format!("Run queued to {target}");
            let _ = db
                .update_automation(
                    &automation.id,
                    crate::storage::AutomationUpdate {
                        status: Some("active".to_string()),
                        last_result: Some(Some(result_msg.clone())),
                        touch_last_run: true,
                        ..Default::default()
                    },
                )
                .await;
            Ok(Json(RunAutomationResponse {
                run_id,
                status: "queued".to_string(),
                message: result_msg,
            }))
        }
        Err(e) => {
            let msg = format!("Failed to enqueue automation run: {e}");
            let _ = db
                .complete_automation_run(&run_id, "error", Some(&msg))
                .await;
            let _ = db
                .update_automation(
                    &automation.id,
                    crate::storage::AutomationUpdate {
                        status: Some("error".to_string()),
                        last_result: Some(Some(msg.clone())),
                        touch_last_run: true,
                        ..Default::default()
                    },
                )
                .await;
            Ok(Json(RunAutomationResponse {
                run_id,
                status: "error".to_string(),
                message: msg,
            }))
        }
    }
}

// --- Generate Automation Flow (LLM-based) ---

async fn generate_automation_flow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GenerateFlowRequest>,
) -> Result<Json<GenerateFlowResponse>, (StatusCode, String)> {
    let desc = req.description.trim();
    if desc.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Description cannot be empty".to_string(),
        ));
    }

    let config = state.config.read().await.clone();

    let system_prompt = r#"You are an automation flow designer. Given a user description, output ONLY valid JSON (no markdown, no explanation) describing an automation flow.

Available node kinds: trigger, tool, skill, mcp, llm, condition, parallel, subprocess, loop, transform, approve, require_2fa, deliver.

Output format:
{
  "name": "Short automation name",
  "flow": {
    "nodes": [
      {"id": "n1", "kind": "trigger", "label": "Short label", "meta": "optional detail"},
      ...
    ],
    "edges": [
      {"from": "n1", "to": "n2"},
      ...
    ]
  }
}

Rules:
- Always start with exactly one trigger node (kind: "trigger")
- Always end with exactly one deliver node (kind: "deliver")
- CRITICAL: "deliver" is the ONLY way to send results to the user. Telegram, Discord, WhatsApp, CLI, and Web are DELIVERY CHANNELS — always use kind "deliver" for them, NEVER "mcp".
- "mcp" is ONLY for external API services: Gmail (read/send email), GitHub (issues, PRs), Slack (channels), Google Calendar, databases, etc. MCP connects to third-party APIs, NOT to messaging channels.
- Use "llm" for agent tasks (summarize, analyze, write, reason, draft, etc.)
- Use "tool" for built-in tools (web_search, shell, file_read, file_write)
- Use "condition" for if/else branching
- Use "transform" for data formatting/filtering between steps
- Use "parallel" to run multiple branches simultaneously (e.g. fetch from 3 sources at once)
- Use "loop" to repeat steps until a condition is met (e.g. retry, paginate)
- Use "subprocess" to call another saved automation as a sub-workflow
- Use "approve" before sensitive steps to require explicit user approval (label = approval question, meta = channel like "telegram:default")
- Use "require_2fa" before critical steps to require two-factor authentication verification
- Keep flows simple: 3-6 nodes typically
- Node IDs should be n1, n2, n3, etc.
- Wire edges sequentially from trigger to deliver

Example: "check emails every morning and send summary to Telegram"
→ trigger(daily 8:00) → mcp(gmail read) → llm(summarize) → deliver(telegram)"#;

    let response = crate::provider::llm_one_shot(
        &config,
        crate::provider::OneShotRequest {
            system_prompt: system_prompt.to_string(),
            user_message: desc.to_string(),
            max_tokens: 4096,
            timeout_secs: 60,
            ..Default::default()
        },
    )
    .await
    .map_err(|e| {
        let status = if e.to_string().contains("timed out") {
            StatusCode::GATEWAY_TIMEOUT
        } else if e.to_string().contains("No active model") || e.to_string().contains("provider") {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, format!("{e}"))
    })?;

    let json_str = extract_json_object_block(&response.content).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Could not extract JSON from LLM response".to_string(),
    ))?;

    let parsed: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid JSON from LLM: {e}"),
        )
    })?;

    let name = parsed
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("New Automation")
        .to_string();
    let flow = parsed.get("flow").cloned().unwrap_or(parsed.clone());

    Ok(Json(GenerateFlowResponse { name, flow }))
}
