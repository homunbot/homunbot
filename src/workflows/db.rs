//! Database operations for the workflow engine.

use anyhow::{Context, Result};
use chrono::Utc;

use crate::storage::Database;

use super::{StepStatus, Workflow, WorkflowCreateRequest, WorkflowStatus, WorkflowStep};

impl Database {
    // ── Workflow CRUD ────────────────────────────────────────────────

    /// Insert a new workflow and its steps. Returns the workflow ID.
    pub async fn insert_workflow(
        &self,
        id: &str,
        req: &WorkflowCreateRequest,
        created_by: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let status = WorkflowStatus::Pending.as_str();
        let deliver_to = req.deliver_to.as_deref();

        sqlx::query(
            "INSERT INTO workflows (id, name, objective, status, created_by, deliver_to, context_json, current_step_idx, created_at)
             VALUES (?, ?, ?, ?, ?, ?, '{}', 0, ?)",
        )
        .bind(id)
        .bind(&req.name)
        .bind(&req.objective)
        .bind(status)
        .bind(created_by)
        .bind(deliver_to)
        .bind(&now)
        .execute(self.pool())
        .await
        .with_context(|| format!("Failed to insert workflow {id}"))?;

        for (idx, step_def) in req.steps.iter().enumerate() {
            let step_id = format!("{id}-s{idx}");
            sqlx::query(
                "INSERT INTO workflow_steps (id, workflow_id, idx, name, instruction, status, approval_required, max_retries)
                 VALUES (?, ?, ?, ?, ?, 'pending', ?, ?)",
            )
            .bind(&step_id)
            .bind(id)
            .bind(idx as i64)
            .bind(&step_def.name)
            .bind(&step_def.instruction)
            .bind(step_def.approval_required)
            .bind(step_def.max_retries as i64)
            .execute(self.pool())
            .await
            .with_context(|| format!("Failed to insert workflow step {step_id}"))?;
        }

        Ok(())
    }

    /// Load a workflow with all its steps.
    pub async fn load_workflow(&self, id: &str) -> Result<Option<Workflow>> {
        let row = sqlx::query_as::<_, WorkflowRow>(
            "SELECT id, name, objective, status, created_by, deliver_to, context_json,
                    current_step_idx, created_at, updated_at, completed_at, error
             FROM workflows WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .with_context(|| format!("Failed to load workflow {id}"))?;

        let Some(row) = row else { return Ok(None) };

        let steps = self.load_workflow_steps(id).await?;
        Ok(Some(row.into_workflow(steps)))
    }

    /// Load steps for a workflow, ordered by idx.
    async fn load_workflow_steps(&self, workflow_id: &str) -> Result<Vec<WorkflowStep>> {
        let rows = sqlx::query_as::<_, StepRow>(
            "SELECT id, workflow_id, idx, name, instruction, status, approval_required,
                    result, error, started_at, completed_at, retry_count, max_retries
             FROM workflow_steps WHERE workflow_id = ? ORDER BY idx",
        )
        .bind(workflow_id)
        .fetch_all(self.pool())
        .await
        .with_context(|| format!("Failed to load steps for workflow {workflow_id}"))?;

        Ok(rows.into_iter().map(|r| r.into_step()).collect())
    }

    /// List workflows, optionally filtered by status.
    pub async fn list_workflows(&self, status_filter: Option<&str>) -> Result<Vec<Workflow>> {
        let rows = if let Some(status) = status_filter {
            sqlx::query_as::<_, WorkflowRow>(
                "SELECT id, name, objective, status, created_by, deliver_to, context_json,
                        current_step_idx, created_at, updated_at, completed_at, error
                 FROM workflows WHERE status = ? ORDER BY created_at DESC",
            )
            .bind(status)
            .fetch_all(self.pool())
            .await?
        } else {
            sqlx::query_as::<_, WorkflowRow>(
                "SELECT id, name, objective, status, created_by, deliver_to, context_json,
                        current_step_idx, created_at, updated_at, completed_at, error
                 FROM workflows ORDER BY created_at DESC",
            )
            .fetch_all(self.pool())
            .await?
        };

        let mut workflows = Vec::with_capacity(rows.len());
        for row in rows {
            let steps = self.load_workflow_steps(&row.id).await?;
            workflows.push(row.into_workflow(steps));
        }
        Ok(workflows)
    }

    /// Load workflows that should be resumed on startup (running or paused).
    pub async fn load_resumable_workflows(&self) -> Result<Vec<Workflow>> {
        let rows = sqlx::query_as::<_, WorkflowRow>(
            "SELECT id, name, objective, status, created_by, deliver_to, context_json,
                    current_step_idx, created_at, updated_at, completed_at, error
             FROM workflows WHERE status IN ('running', 'pending')
             ORDER BY created_at ASC",
        )
        .fetch_all(self.pool())
        .await?;

        let mut workflows = Vec::with_capacity(rows.len());
        for row in rows {
            let steps = self.load_workflow_steps(&row.id).await?;
            workflows.push(row.into_workflow(steps));
        }
        Ok(workflows)
    }

    // ── Status updates ──────────────────────────────────────────────

    pub async fn update_workflow_status(
        &self,
        id: &str,
        status: WorkflowStatus,
        error: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let completed_at = if status.is_terminal() {
            Some(now.clone())
        } else {
            None
        };

        sqlx::query(
            "UPDATE workflows SET status = ?, error = ?, updated_at = ?, completed_at = COALESCE(?, completed_at)
             WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(error)
        .bind(&now)
        .bind(completed_at.as_deref())
        .bind(id)
        .execute(self.pool())
        .await
        .with_context(|| format!("Failed to update workflow status {id}"))?;

        Ok(())
    }

    pub async fn update_workflow_step_idx(&self, id: &str, step_idx: usize) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE workflows SET current_step_idx = ?, updated_at = ? WHERE id = ?")
            .bind(step_idx as i64)
            .bind(&now)
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn update_workflow_context(
        &self,
        id: &str,
        context: &serde_json::Value,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let json = serde_json::to_string(context)?;
        sqlx::query("UPDATE workflows SET context_json = ?, updated_at = ? WHERE id = ?")
            .bind(&json)
            .bind(&now)
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn update_step_status(
        &self,
        step_id: &str,
        status: StepStatus,
        result: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let started_at = if status == StepStatus::Running {
            Some(now.clone())
        } else {
            None
        };
        let completed_at = if matches!(status, StepStatus::Completed | StepStatus::Failed | StepStatus::Skipped) {
            Some(now.clone())
        } else {
            None
        };

        sqlx::query(
            "UPDATE workflow_steps SET status = ?, result = COALESCE(?, result), error = COALESCE(?, error),
             started_at = COALESCE(?, started_at), completed_at = COALESCE(?, completed_at)
             WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(result)
        .bind(error)
        .bind(started_at.as_deref())
        .bind(completed_at.as_deref())
        .bind(step_id)
        .execute(self.pool())
        .await
        .with_context(|| format!("Failed to update step {step_id}"))?;

        Ok(())
    }

    pub async fn increment_step_retry(&self, step_id: &str) -> Result<()> {
        sqlx::query("UPDATE workflow_steps SET retry_count = retry_count + 1 WHERE id = ?")
            .bind(step_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Cancel all pending steps of a workflow (for cancel/fail operations).
    pub async fn cancel_pending_steps(&self, workflow_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE workflow_steps SET status = 'skipped', completed_at = ?
             WHERE workflow_id = ? AND status IN ('pending', 'running')",
        )
        .bind(&now)
        .bind(workflow_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}

// ── SQLx row types ──────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct WorkflowRow {
    id: String,
    name: String,
    objective: String,
    status: String,
    created_by: Option<String>,
    deliver_to: Option<String>,
    context_json: String,
    current_step_idx: i64,
    created_at: String,
    updated_at: Option<String>,
    completed_at: Option<String>,
    error: Option<String>,
}

impl WorkflowRow {
    fn into_workflow(self, steps: Vec<WorkflowStep>) -> Workflow {
        Workflow {
            id: self.id,
            name: self.name,
            objective: self.objective,
            status: WorkflowStatus::from_str(&self.status),
            steps,
            context: serde_json::from_str(&self.context_json).unwrap_or_default(),
            created_by: self.created_by,
            deliver_to: self.deliver_to,
            current_step_idx: self.current_step_idx as usize,
            created_at: self.created_at,
            updated_at: self.updated_at,
            completed_at: self.completed_at,
            error: self.error,
        }
    }
}

#[derive(sqlx::FromRow)]
struct StepRow {
    id: String,
    workflow_id: String,
    idx: i64,
    name: String,
    instruction: String,
    status: String,
    approval_required: bool,
    result: Option<String>,
    error: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    retry_count: i64,
    max_retries: i64,
}

impl StepRow {
    fn into_step(self) -> WorkflowStep {
        WorkflowStep {
            id: self.id,
            workflow_id: self.workflow_id,
            idx: self.idx as usize,
            name: self.name,
            instruction: self.instruction,
            status: StepStatus::from_str(&self.status),
            approval_required: self.approval_required,
            result: self.result,
            error: self.error,
            started_at: self.started_at,
            completed_at: self.completed_at,
            retry_count: self.retry_count as u32,
            max_retries: self.max_retries as u32,
        }
    }
}
