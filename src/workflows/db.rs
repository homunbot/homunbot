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
            "INSERT INTO workflows (id, name, objective, status, created_by, deliver_to, automation_id, automation_run_id, context_json, current_step_idx, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, '{}', 0, ?)",
        )
        .bind(id)
        .bind(&req.name)
        .bind(&req.objective)
        .bind(status)
        .bind(created_by)
        .bind(deliver_to)
        .bind(req.automation_id.as_deref())
        .bind(req.automation_run_id.as_deref())
        .bind(&now)
        .execute(self.pool())
        .await
        .with_context(|| format!("Failed to insert workflow {id}"))?;

        for (idx, step_def) in req.steps.iter().enumerate() {
            let step_id = format!("{id}-s{idx}");
            let agent_id = step_def.agent_id.as_deref().unwrap_or("default");
            sqlx::query(
                "INSERT INTO workflow_steps (id, workflow_id, idx, name, instruction, status, approval_required, max_retries, agent_id)
                 VALUES (?, ?, ?, ?, ?, 'pending', ?, ?, ?)",
            )
            .bind(&step_id)
            .bind(id)
            .bind(idx as i64)
            .bind(&step_def.name)
            .bind(&step_def.instruction)
            .bind(step_def.approval_required)
            .bind(step_def.max_retries as i64)
            .bind(agent_id)
            .execute(self.pool())
            .await
            .with_context(|| format!("Failed to insert workflow step {step_id}"))?;
        }

        Ok(())
    }

    /// Load a workflow with all its steps.
    pub async fn load_workflow(&self, id: &str) -> Result<Option<Workflow>> {
        let row = sqlx::query_as::<_, WorkflowRow>(
            "SELECT id, name, objective, status, created_by, deliver_to, automation_id, automation_run_id, context_json,
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
                    result, error, started_at, completed_at, retry_count, max_retries, agent_id
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
                "SELECT id, name, objective, status, created_by, deliver_to, automation_id, automation_run_id, context_json,
                        current_step_idx, created_at, updated_at, completed_at, error
                 FROM workflows WHERE status = ? ORDER BY created_at DESC",
            )
            .bind(status)
            .fetch_all(self.pool())
            .await?
        } else {
            sqlx::query_as::<_, WorkflowRow>(
                "SELECT id, name, objective, status, created_by, deliver_to, automation_id, automation_run_id, context_json,
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
            "SELECT id, name, objective, status, created_by, deliver_to, automation_id, automation_run_id, context_json,
                    current_step_idx, created_at, updated_at, completed_at, error
             FROM workflows WHERE status IN ('running', 'pending', 'paused')
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
        let completed_at = if matches!(
            status,
            StepStatus::Completed | StepStatus::Failed | StepStatus::Skipped
        ) {
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

    /// Delete a workflow and all its steps.
    pub async fn delete_workflow(&self, workflow_id: &str) -> Result<()> {
        // Steps are deleted by ON DELETE CASCADE
        sqlx::query("DELETE FROM workflows WHERE id = ?")
            .bind(workflow_id)
            .execute(self.pool())
            .await
            .with_context(|| format!("Failed to delete workflow {workflow_id}"))?;
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
    automation_id: Option<String>,
    automation_run_id: Option<String>,
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
            automation_id: self.automation_id,
            automation_run_id: self.automation_run_id,
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
    agent_id: Option<String>,
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
            agent_id: self.agent_id.unwrap_or_else(|| "default".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::Database;
    use crate::workflows::{StepDefinition, WorkflowCreateRequest, WorkflowStatus};

    /// Helper to create a test DB with migrations applied.
    async fn test_db() -> Database {
        let dir = tempfile::tempdir().expect("tempdir");
        Database::open(&dir.path().join("test.db"))
            .await
            .expect("open test db")
    }

    fn sample_request() -> WorkflowCreateRequest {
        WorkflowCreateRequest {
            name: "Test Workflow".to_string(),
            objective: "Testing".to_string(),
            steps: vec![StepDefinition {
                name: "Step 1".to_string(),
                instruction: "Do something".to_string(),
                approval_required: true,
                max_retries: 1,
                agent_id: None,
            }],
            deliver_to: None,
            automation_id: None,
            automation_run_id: None,
        }
    }

    #[tokio::test]
    async fn test_load_resumable_includes_paused() {
        let db = test_db().await;

        // Create and insert 3 workflows with different statuses
        let req = sample_request();
        db.insert_workflow("wf-running", &req, None).await.unwrap();
        db.update_workflow_status("wf-running", WorkflowStatus::Running, None)
            .await
            .unwrap();

        db.insert_workflow("wf-paused", &req, None).await.unwrap();
        db.update_workflow_status("wf-paused", WorkflowStatus::Paused, None)
            .await
            .unwrap();

        db.insert_workflow("wf-completed", &req, None).await.unwrap();
        db.update_workflow_status("wf-completed", WorkflowStatus::Completed, None)
            .await
            .unwrap();

        db.insert_workflow("wf-pending", &req, None).await.unwrap();
        // pending is the default status — no update needed

        let resumable = db.load_resumable_workflows().await.unwrap();
        let ids: Vec<&str> = resumable.iter().map(|w| w.id.as_str()).collect();

        assert!(ids.contains(&"wf-running"), "running should be resumable");
        assert!(ids.contains(&"wf-paused"), "paused should be resumable");
        assert!(ids.contains(&"wf-pending"), "pending should be resumable");
        assert!(
            !ids.contains(&"wf-completed"),
            "completed should NOT be resumable"
        );
    }

    #[tokio::test]
    async fn test_workflow_insert_and_load_roundtrip() {
        let db = test_db().await;
        let req = sample_request();
        db.insert_workflow("wf-rt", &req, Some("web:test"))
            .await
            .unwrap();

        let wf = db.load_workflow("wf-rt").await.unwrap().unwrap();
        assert_eq!(wf.name, "Test Workflow");
        assert_eq!(wf.objective, "Testing");
        assert_eq!(wf.status, WorkflowStatus::Pending);
        assert_eq!(wf.steps.len(), 1);
        assert!(wf.steps[0].approval_required);
        assert_eq!(wf.created_by.as_deref(), Some("web:test"));
        assert!(wf.automation_id.is_none());
    }

    #[tokio::test]
    async fn test_workflow_automation_link_roundtrip() {
        let db = test_db().await;

        // Create a parent automation first (FK constraint)
        db.insert_automation(
            "auto-123",
            "Test Auto",
            "do stuff",
            "every:3600",
            true,
            "active",
            None,
            "always",
            None,
        )
        .await
        .unwrap();

        let mut req = sample_request();
        req.automation_id = Some("auto-123".to_string());
        req.automation_run_id = Some("run-456".to_string());

        db.insert_workflow("wf-linked", &req, None).await.unwrap();

        let wf = db.load_workflow("wf-linked").await.unwrap().unwrap();
        assert_eq!(wf.automation_id.as_deref(), Some("auto-123"));
        assert_eq!(wf.automation_run_id.as_deref(), Some("run-456"));
    }

    #[tokio::test]
    async fn test_workflow_without_automation_link() {
        let db = test_db().await;
        let req = sample_request();
        db.insert_workflow("wf-solo", &req, None).await.unwrap();

        let wf = db.load_workflow("wf-solo").await.unwrap().unwrap();
        assert!(wf.automation_id.is_none());
        assert!(wf.automation_run_id.is_none());
    }
}
