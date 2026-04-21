//! AI Agent Workflow Orchestration client — wraps `/agent-workflows/*` routes (#2915).
//!
//! Distinct from marketplace workflows. See monorepo issue #2915 for the full
//! architecture. A workflow is a directed acyclic graph of agent nodes where
//! each node invokes an agent from the tenant's registry. Triggers can be
//! manual, cron-based, or event-driven.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Tenant-scoped multi-agent DAG workflow service (#2915).
///
/// Free tier: 100 executions, 1000 agent messages, 10k D1 queries per month.
pub struct AgentWorkflowsService {
    http: Arc<OlympusHttpClient>,
}

/// Filter options for `list()`.
#[derive(Default)]
pub struct ListWorkflowsOptions<'a> {
    /// Filter by "draft", "active", "paused", "archived".
    pub status: Option<&'a str>,
    pub limit: Option<u32>,
}

/// Parameters for `create()`.
pub struct CreateWorkflowRequest<'a> {
    pub name: &'a str,
    pub description: Option<&'a str>,
    /// DAG definition with `nodes` and `edges`.
    pub schema: Value,
    /// Optional list of trigger configs (cron/event/manual).
    pub triggers: Option<Value>,
}

/// Filter options for `list_executions()`.
#[derive(Default)]
pub struct ListExecutionsOptions<'a> {
    pub status: Option<&'a str>,
    pub limit: Option<u32>,
}

impl AgentWorkflowsService {
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// List workflows for the current tenant.
    pub async fn list(&self, opts: ListWorkflowsOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(s) = opts.status {
            query.push(("status", s.to_string()));
        }
        if let Some(l) = opts.limit {
            query.push(("limit", l.to_string()));
        }
        let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.http
            .get_with_query("/agent-workflows", &query_refs)
            .await
    }

    /// Get a single workflow by ID with its full DAG schema.
    pub async fn get(&self, workflow_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/agent-workflows/{}", workflow_id))
            .await
    }

    /// Create a new workflow.
    pub async fn create(&self, req: CreateWorkflowRequest<'_>) -> Result<Value> {
        let mut body = json!({
            "name": req.name,
            "schema": req.schema,
        });
        if let Some(d) = req.description {
            body["description"] = Value::String(d.to_string());
        }
        if let Some(t) = req.triggers {
            body["triggers"] = t;
        }
        self.http.post("/agent-workflows", &body).await
    }

    /// Update an existing workflow. Pass only fields to change.
    pub async fn update(&self, workflow_id: &str, updates: Value) -> Result<Value> {
        self.http
            .put(&format!("/agent-workflows/{}", workflow_id), &updates)
            .await
    }

    /// Soft-delete (archive) a workflow.
    pub async fn delete(&self, workflow_id: &str) -> Result<()> {
        self.http
            .delete(&format!("/agent-workflows/{}", workflow_id))
            .await?;
        Ok(())
    }

    /// Manually trigger a workflow execution. Returns execution ID — poll
    /// `get_execution` for results.
    pub async fn execute(&self, workflow_id: &str, input: Option<Value>) -> Result<Value> {
        let body = match input {
            Some(v) => json!({ "input": v }),
            None => json!({}),
        };
        self.http
            .post(&format!("/agent-workflows/{}/execute", workflow_id), &body)
            .await
    }

    /// List execution history for a workflow.
    pub async fn list_executions(
        &self,
        workflow_id: &str,
        opts: ListExecutionsOptions<'_>,
    ) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(s) = opts.status {
            query.push(("status", s.to_string()));
        }
        if let Some(l) = opts.limit {
            query.push(("limit", l.to_string()));
        }
        let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.http
            .get_with_query(
                &format!("/agent-workflows/{}/executions", workflow_id),
                &query_refs,
            )
            .await
    }

    /// Get full execution detail including per-step results.
    pub async fn get_execution(&self, execution_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/agent-workflow-executions/{}", execution_id))
            .await
    }

    /// Set or update the cron schedule for a workflow. Cron expression follows
    /// the standard five-field format: `minute hour day month weekday`.
    pub async fn set_schedule(&self, workflow_id: &str, cron_expression: &str) -> Result<Value> {
        let body = json!({ "cron_expression": cron_expression });
        self.http
            .post(&format!("/agent-workflows/{}/schedule", workflow_id), &body)
            .await
    }

    /// Remove the cron schedule from a workflow.
    pub async fn remove_schedule(&self, workflow_id: &str) -> Result<()> {
        self.http
            .delete(&format!("/agent-workflows/{}/schedule", workflow_id))
            .await?;
        Ok(())
    }

    /// Get current month usage vs tenant tier limits.
    pub async fn usage(&self) -> Result<Value> {
        self.http.get("/agent-workflows/usage").await
    }
}
