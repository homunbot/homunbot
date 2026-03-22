//! Database operations for the automations subsystem.
//!
//! Extension `impl Database` for automation CRUD + run tracking.
//! Follows the pattern in `business/db.rs` and `contacts/db.rs`.

use anyhow::{Context, Result};

use crate::storage::{AutomationRow, AutomationRunRow, AutomationUpdate, Database};

impl Database {
    /// Insert a new automation definition.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_automation(
        &self,
        id: &str,
        name: &str,
        prompt: &str,
        schedule: &str,
        enabled: bool,
        status: &str,
        deliver_to: Option<&str>,
        trigger_kind: &str,
        trigger_value: Option<&str>,
    ) -> Result<()> {
        self.insert_automation_with_plan(
            id,
            name,
            prompt,
            schedule,
            enabled,
            status,
            deliver_to,
            trigger_kind,
            trigger_value,
            None,
            "[]",
            1,
            None,
        )
        .await
    }

    /// Insert a new automation definition with compiled plan metadata.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_automation_with_plan(
        &self,
        id: &str,
        name: &str,
        prompt: &str,
        schedule: &str,
        enabled: bool,
        status: &str,
        deliver_to: Option<&str>,
        trigger_kind: &str,
        trigger_value: Option<&str>,
        plan_json: Option<&str>,
        dependencies_json: &str,
        plan_version: i64,
        validation_errors: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO automations
                 (id, name, prompt, schedule, enabled, status, deliver_to, trigger_kind, trigger_value,
                  plan_json, dependencies_json, plan_version, validation_errors, profile_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(prompt)
        .bind(schedule)
        .bind(enabled)
        .bind(status)
        .bind(deliver_to)
        .bind(trigger_kind)
        .bind(trigger_value)
        .bind(plan_json)
        .bind(dependencies_json)
        .bind(plan_version)
        .bind(validation_errors)
        .bind(Option::<i64>::None) // profile_id set via API or tool context
        .execute(self.pool())
        .await
        .context("Failed to insert automation")?;

        Ok(())
    }

    /// Load all automations.
    pub async fn load_automations(&self) -> Result<Vec<AutomationRow>> {
        let rows = sqlx::query_as::<_, AutomationRow>(
            "SELECT id, name, prompt, schedule, enabled, status, deliver_to,
                    trigger_kind, trigger_value,
                    last_run, last_result, created_at, updated_at,
                    plan_json, dependencies_json, plan_version, validation_errors,
                    workflow_steps_json, flow_json, profile_id
             FROM automations
             ORDER BY created_at DESC",
        )
        .fetch_all(self.pool())
        .await
        .context("Failed to load automations")?;

        Ok(rows)
    }

    /// Load one automation by ID.
    pub async fn load_automation(&self, id: &str) -> Result<Option<AutomationRow>> {
        let row = sqlx::query_as::<_, AutomationRow>(
            "SELECT id, name, prompt, schedule, enabled, status, deliver_to,
                    trigger_kind, trigger_value,
                    last_run, last_result, created_at, updated_at,
                    plan_json, dependencies_json, plan_version, validation_errors,
                    workflow_steps_json, flow_json, profile_id
             FROM automations
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .context("Failed to load automation")?;

        Ok(row)
    }

    /// Apply a partial update to an automation.
    pub async fn update_automation(&self, id: &str, update: AutomationUpdate) -> Result<bool> {
        let Some(current) = self.load_automation(id).await? else {
            return Ok(false);
        };

        let name = update.name.unwrap_or(current.name);
        let prompt = update.prompt.unwrap_or(current.prompt);
        let schedule = update.schedule.unwrap_or(current.schedule);
        let enabled = update.enabled.unwrap_or(current.enabled);
        let status = update.status.unwrap_or(current.status);
        let deliver_to = update.deliver_to.unwrap_or(current.deliver_to);
        let trigger_kind = update.trigger_kind.unwrap_or(current.trigger_kind);
        let trigger_value = update.trigger_value.unwrap_or(current.trigger_value);
        let last_result = update.last_result.unwrap_or(current.last_result);
        let plan_json = match update.plan_json {
            Some(v) => v,
            None => current.plan_json,
        };
        let dependencies_json = match update.dependencies_json {
            Some(Some(v)) => v,
            Some(None) => "[]".to_string(),
            None => current.dependencies_json,
        };
        let plan_version = update.plan_version.unwrap_or(current.plan_version);
        let validation_errors = match update.validation_errors {
            Some(v) => v,
            None => current.validation_errors,
        };
        let workflow_steps_json = match update.workflow_steps_json {
            Some(v) => v,
            None => current.workflow_steps_json,
        };
        let flow_json = match update.flow_json {
            Some(v) => v,
            None => current.flow_json,
        };

        let result = sqlx::query(
            "UPDATE automations
             SET name = ?, prompt = ?, schedule = ?, enabled = ?, status = ?,
                 deliver_to = ?, trigger_kind = ?, trigger_value = ?, last_result = ?,
                 plan_json = ?, dependencies_json = ?, plan_version = ?, validation_errors = ?,
                 workflow_steps_json = ?, flow_json = ?,
                 last_run = CASE WHEN ? THEN datetime('now') ELSE last_run END,
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(name)
        .bind(prompt)
        .bind(schedule)
        .bind(enabled)
        .bind(status)
        .bind(deliver_to)
        .bind(trigger_kind)
        .bind(trigger_value)
        .bind(last_result)
        .bind(plan_json)
        .bind(dependencies_json)
        .bind(plan_version)
        .bind(validation_errors)
        .bind(workflow_steps_json)
        .bind(flow_json)
        .bind(update.touch_last_run)
        .bind(id)
        .execute(self.pool())
        .await
        .context("Failed to update automation")?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete an automation.
    pub async fn delete_automation(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM automations WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .context("Failed to delete automation")?;

        Ok(result.rows_affected() > 0)
    }

    /// Mark automations as invalid when a dependency is removed.
    ///
    /// Returns number of automations updated.
    pub async fn invalidate_automations_by_dependency(
        &self,
        dependency_kind: &str,
        dependency_name: &str,
        reason: &str,
    ) -> Result<u64> {
        let rows = self.load_automations().await?;
        let mut affected = 0_u64;

        for row in rows {
            if !crate::scheduler::automations::dependencies_include(
                &row.dependencies_json,
                dependency_kind,
                dependency_name,
            ) {
                continue;
            }

            let mut errors = crate::scheduler::automations::parse_validation_errors_json(
                row.validation_errors.as_deref(),
            );
            if !errors.iter().any(|e| e == reason) {
                errors.push(reason.to_string());
            }
            let errors_json = serde_json::to_string(&errors).unwrap_or_else(|_| "[]".to_string());

            let changed = self
                .update_automation(
                    &row.id,
                    AutomationUpdate {
                        status: Some("invalid_config".to_string()),
                        validation_errors: Some(Some(errors_json)),
                        last_result: Some(Some(format!("Automation invalidated: {reason}"))),
                        ..Default::default()
                    },
                )
                .await?;
            if changed {
                affected += 1;
            }
        }

        Ok(affected)
    }

    /// Insert a new automation run.
    pub async fn insert_automation_run(
        &self,
        id: &str,
        automation_id: &str,
        status: &str,
        result: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO automation_runs (id, automation_id, status, result)
             VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(automation_id)
        .bind(status)
        .bind(result)
        .execute(self.pool())
        .await
        .context("Failed to insert automation run")?;
        Ok(())
    }

    /// Complete an automation run with final status/result.
    pub async fn complete_automation_run(
        &self,
        run_id: &str,
        status: &str,
        result: Option<&str>,
    ) -> Result<bool> {
        let changed = sqlx::query(
            "UPDATE automation_runs
             SET status = ?, result = ?, finished_at = datetime('now')
             WHERE id = ?",
        )
        .bind(status)
        .bind(result)
        .bind(run_id)
        .execute(self.pool())
        .await
        .context("Failed to complete automation run")?;
        Ok(changed.rows_affected() > 0)
    }

    /// Load run history for an automation (latest first).
    pub async fn load_automation_runs(
        &self,
        automation_id: &str,
        limit: u32,
    ) -> Result<Vec<AutomationRunRow>> {
        let rows = sqlx::query_as::<_, AutomationRunRow>(
            "SELECT id, automation_id, started_at, finished_at, status, result
             FROM automation_runs
             WHERE automation_id = ?
             ORDER BY started_at DESC
             LIMIT ?",
        )
        .bind(automation_id)
        .bind(limit as i64)
        .fetch_all(self.pool())
        .await
        .context("Failed to load automation runs")?;
        Ok(rows)
    }

    /// Load latest successful run result for an automation.
    /// Optionally excludes a run ID (useful when finalizing that same run).
    pub async fn load_last_successful_automation_result(
        &self,
        automation_id: &str,
        exclude_run_id: Option<&str>,
    ) -> Result<Option<String>> {
        let row = sqlx::query_scalar::<_, String>(
            "SELECT result
             FROM automation_runs
             WHERE automation_id = ?
               AND status = 'success'
               AND result IS NOT NULL
               AND (? IS NULL OR id <> ?)
             ORDER BY started_at DESC
             LIMIT 1",
        )
        .bind(automation_id)
        .bind(exclude_run_id)
        .bind(exclude_run_id)
        .fetch_optional(self.pool())
        .await
        .context("Failed to load last successful automation result")?;

        Ok(row)
    }
}
