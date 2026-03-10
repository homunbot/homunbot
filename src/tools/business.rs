//! LLM tool for autonomous business management.
//!
//! 13 actions: launch, list, status, research, strategize, create_product,
//! record_sale, record_expense, revenue, review, pivot, pause, close.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::registry::{Tool, ToolContext, ToolResult};
use crate::business::engine::BusinessEngine;
use crate::business::{BusinessAutonomy, FiscalConfig, StrategyStatus};

pub struct BusinessTool {
    engine: Arc<tokio::sync::OnceCell<Arc<BusinessEngine>>>,
}

impl BusinessTool {
    pub fn new(engine: Arc<tokio::sync::OnceCell<Arc<BusinessEngine>>>) -> Self {
        Self { engine }
    }

    fn get_engine(&self) -> Result<&BusinessEngine> {
        self.engine
            .get()
            .map(|e| e.as_ref())
            .ok_or_else(|| anyhow::anyhow!("Business engine not initialized yet"))
    }
}

#[async_trait]
impl Tool for BusinessTool {
    fn name(&self) -> &str {
        "business"
    }

    fn description(&self) -> &str {
        "Manage autonomous businesses: launch ventures, track revenue, create products, \
         research markets, develop strategies, and record sales/expenses. Each business \
         has an OODA review loop that periodically assesses strategy and performance. \
         Supports semi-autonomous (propose first), budget-limited, and fully autonomous modes."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["launch", "list", "status", "research", "strategize",
                             "create_product", "record_sale", "record_expense",
                             "revenue", "review", "pivot", "pause", "close"],
                    "description": "Action to perform"
                },
                "business_id": {
                    "type": "string",
                    "description": "Business ID (required for most actions except launch/list)"
                },
                "name": {
                    "type": "string",
                    "description": "Name (for launch, strategize, create_product)"
                },
                "description": {
                    "type": "string",
                    "description": "Description (for launch, create_product)"
                },
                "autonomy": {
                    "type": "string",
                    "enum": ["semi", "budget", "full"],
                    "description": "Autonomy level (for launch). Default: semi"
                },
                "budget": {
                    "type": "number",
                    "description": "Budget limit (for launch with autonomy=budget)"
                },
                "currency": {
                    "type": "string",
                    "description": "Currency code (default: EUR)"
                },
                "deliver_to": {
                    "type": "string",
                    "description": "Channel:chat_id for notifications (for launch)"
                },
                "topic": {
                    "type": "string",
                    "description": "Research topic (for research)"
                },
                "insight_type": {
                    "type": "string",
                    "enum": ["research", "competitor", "trend", "opportunity"],
                    "description": "Type of insight (for research). Default: research"
                },
                "hypothesis": {
                    "type": "string",
                    "description": "Strategy hypothesis (for strategize, pivot)"
                },
                "approach": {
                    "type": "string",
                    "description": "Strategy approach (for strategize)"
                },
                "product_type": {
                    "type": "string",
                    "enum": ["digital", "physical", "service", "subscription"],
                    "description": "Product type (for create_product). Default: digital"
                },
                "price": {
                    "type": "number",
                    "description": "Product price (for create_product)"
                },
                "amount": {
                    "type": "number",
                    "description": "Amount (for record_sale, record_expense)"
                },
                "category": {
                    "type": "string",
                    "description": "Expense category (for record_expense)"
                },
                "product_id": {
                    "type": "string",
                    "description": "Product ID (for record_sale)"
                },
                "from_strategy_id": {
                    "type": "string",
                    "description": "Strategy to pivot from (for pivot)"
                },
                "source": {
                    "type": "string",
                    "description": "Revenue source (for record_sale)"
                },
                "tax_amount": {
                    "type": "number",
                    "description": "Tax amount (for record_sale)"
                },
                "tax_rate": {
                    "type": "number",
                    "description": "Tax rate percentage (for record_sale)"
                },
                "content": {
                    "type": "string",
                    "description": "Insight content (for research)"
                },
                "confidence": {
                    "type": "number",
                    "description": "Confidence 0.0-1.0 (for research)"
                },
                "filter": {
                    "type": "string",
                    "enum": ["all", "active", "planning", "paused", "closed"],
                    "description": "Status filter (for list). Default: all"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        let engine = match self.get_engine() {
            Ok(e) => e,
            Err(e) => return Ok(ToolResult::error(format!("{e}"))),
        };

        match action {
            "launch" => self.handle_launch(engine, &args, ctx).await,
            "list" => self.handle_list(engine, &args).await,
            "status" => self.handle_status(engine, &args).await,
            "research" => self.handle_research(engine, &args).await,
            "strategize" => self.handle_strategize(engine, &args, ctx).await,
            "create_product" => self.handle_create_product(engine, &args, ctx).await,
            "record_sale" => self.handle_record_sale(engine, &args).await,
            "record_expense" => self.handle_record_expense(engine, &args).await,
            "revenue" => self.handle_revenue(engine, &args).await,
            "review" => self.handle_review(engine, &args).await,
            "pivot" => self.handle_pivot(engine, &args, ctx).await,
            "pause" => self.handle_pause(engine, &args).await,
            "close" => self.handle_close(engine, &args).await,
            _ => Ok(ToolResult::error(format!("Unknown action: {action}"))),
        }
    }
}

/// Serialize JSON value to pretty string for tool output.
fn json_output(v: Value) -> String {
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string())
}

// ── Action handlers ──────────────────────────────────────────────────

impl BusinessTool {
    async fn handle_launch(
        &self,
        engine: &BusinessEngine,
        args: &Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return Ok(ToolResult::error("Missing required parameter: name")),
        };

        let autonomy_str = args
            .get("autonomy")
            .and_then(|v| v.as_str())
            .unwrap_or("semi");
        let autonomy = BusinessAutonomy::from_str(autonomy_str);

        let budget = args.get("budget").and_then(|v| v.as_f64());
        let currency = args
            .get("currency")
            .and_then(|v| v.as_str())
            .unwrap_or("EUR");
        let description = args.get("description").and_then(|v| v.as_str());
        let deliver_to = args.get("deliver_to").and_then(|v| v.as_str());

        let ooda_interval = "every:86400"; // Default 24h

        let created_by = Some(format!("{}:{}", ctx.channel, ctx.chat_id));

        let biz = engine
            .launch(
                name,
                description,
                autonomy,
                budget,
                currency,
                ooda_interval,
                deliver_to,
                Some(&created_by.as_deref().unwrap_or("unknown")),
                None, // fiscal_config set later
            )
            .await?;

        // Build OODA prompt for automation creation
        let ooda_prompt = engine.build_ooda_prompt(&biz);

        Ok(ToolResult::success(json_output(json!({
            "business_id": biz.id,
            "name": biz.name,
            "status": biz.status.as_str(),
            "autonomy": biz.autonomy_level.as_str(),
            "budget": biz.budget_total,
            "currency": biz.budget_currency,
            "ooda_prompt": ooda_prompt,
            "message": format!(
                "Business '{}' launched (id: {}). Create an automation with the ooda_prompt to enable periodic OODA reviews.",
                biz.name, biz.id
            )
        }))))
    }

    async fn handle_list(
        &self,
        engine: &BusinessEngine,
        args: &Value,
    ) -> Result<ToolResult> {
        let filter = args.get("filter").and_then(|v| v.as_str());
        let status_filter = match filter {
            Some("all") | None => None,
            Some(s) => Some(s),
        };

        let businesses = engine.db().list_businesses(status_filter).await?;

        let items: Vec<Value> = businesses
            .iter()
            .map(|b| {
                json!({
                    "id": b.id,
                    "name": b.name,
                    "status": b.status.as_str(),
                    "autonomy": b.autonomy_level.as_str(),
                    "budget_spent": b.budget_spent,
                    "budget_total": b.budget_total,
                    "currency": b.budget_currency,
                })
            })
            .collect();

        Ok(ToolResult::success(json_output(json!({
            "count": items.len(),
            "businesses": items
        }))))
    }

    async fn handle_status(
        &self,
        engine: &BusinessEngine,
        args: &Value,
    ) -> Result<ToolResult> {
        let id = match args.get("business_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required parameter: business_id")),
        };

        let report = engine.status_report(id).await?;
        Ok(ToolResult::success(json_output(json!({ "report": report }))))
    }

    async fn handle_research(
        &self,
        engine: &BusinessEngine,
        args: &Value,
    ) -> Result<ToolResult> {
        let business_id = match args.get("business_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required parameter: business_id")),
        };
        let topic = match args.get("topic").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return Ok(ToolResult::error("Missing required parameter: topic")),
        };
        let content = match args.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return Ok(ToolResult::error("Missing required parameter: content")),
        };

        let insight_type = args
            .get("insight_type")
            .and_then(|v| v.as_str())
            .unwrap_or("research");
        let confidence = args.get("confidence").and_then(|v| v.as_f64());
        let source = args.get("source").and_then(|v| v.as_str());

        let insight = engine
            .add_insight(business_id, topic, insight_type, content, confidence, source)
            .await?;

        Ok(ToolResult::success(json_output(json!({
            "insight_id": insight.id,
            "message": format!("Market insight recorded: {}", topic)
        }))))
    }

    async fn handle_strategize(
        &self,
        engine: &BusinessEngine,
        args: &Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let business_id = match args.get("business_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required parameter: business_id")),
        };
        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return Ok(ToolResult::error("Missing required parameter: name")),
        };
        let hypothesis = match args.get("hypothesis").and_then(|v| v.as_str()) {
            Some(h) => h,
            None => return Ok(ToolResult::error("Missing required parameter: hypothesis")),
        };
        let approach = args.get("approach").and_then(|v| v.as_str());

        // Check autonomy for semi mode
        let biz = engine
            .db()
            .load_business(business_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Business not found"))?;

        let strategy = engine
            .add_strategy(business_id, name, hypothesis, approach)
            .await?;

        if biz.autonomy_level == BusinessAutonomy::Semi {
            Ok(ToolResult::success(json_output(json!({
                "strategy_id": strategy.id,
                "status": "proposed",
                "message": format!(
                    "Strategy '{}' proposed (semi-autonomous mode). Present to user for approval before executing.",
                    name
                )
            }))))
        } else {
            // Auto-approve for budget/full modes
            engine
                .db()
                .update_strategy_status(&strategy.id, StrategyStatus::Active)
                .await?;
            Ok(ToolResult::success(json_output(json!({
                "strategy_id": strategy.id,
                "status": "active",
                "message": format!("Strategy '{}' created and activated.", name)
            }))))
        }
    }

    async fn handle_create_product(
        &self,
        engine: &BusinessEngine,
        args: &Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let business_id = match args.get("business_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required parameter: business_id")),
        };
        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return Ok(ToolResult::error("Missing required parameter: name")),
        };
        let price = args.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let product_type = args
            .get("product_type")
            .and_then(|v| v.as_str())
            .unwrap_or("digital");
        let description = args.get("description").and_then(|v| v.as_str());
        let currency = args
            .get("currency")
            .and_then(|v| v.as_str())
            .unwrap_or("EUR");

        // Check autonomy
        let biz = engine
            .db()
            .load_business(business_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Business not found"))?;

        let product = engine
            .create_product(business_id, name, description, product_type, price, currency)
            .await?;

        if biz.autonomy_level == BusinessAutonomy::Semi {
            Ok(ToolResult::success(json_output(json!({
                "product_id": product.id,
                "status": "draft",
                "message": format!(
                    "Product '{}' created as draft (semi-autonomous). Present to user for approval.",
                    name
                )
            }))))
        } else {
            Ok(ToolResult::success(json_output(json!({
                "product_id": product.id,
                "status": "draft",
                "message": format!("Product '{}' created. Set to active when ready to sell.", name)
            }))))
        }
    }

    async fn handle_record_sale(
        &self,
        engine: &BusinessEngine,
        args: &Value,
    ) -> Result<ToolResult> {
        let business_id = match args.get("business_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required parameter: business_id")),
        };
        let amount = match args.get("amount").and_then(|v| v.as_f64()) {
            Some(a) => a,
            None => return Ok(ToolResult::error("Missing required parameter: amount")),
        };

        let currency = args
            .get("currency")
            .and_then(|v| v.as_str())
            .unwrap_or("EUR");
        let product_id = args.get("product_id").and_then(|v| v.as_str());
        let description = args.get("description").and_then(|v| v.as_str());
        let tax_amount = args.get("tax_amount").and_then(|v| v.as_f64());
        let tax_rate = args.get("tax_rate").and_then(|v| v.as_f64());
        let source = args.get("source").and_then(|v| v.as_str());

        let tx = engine
            .record_sale(
                business_id,
                amount,
                currency,
                product_id,
                description,
                tax_amount,
                tax_rate,
                source,
            )
            .await?;

        Ok(ToolResult::success(json_output(json!({
            "transaction_id": tx.id,
            "message": format!("Sale recorded: {:.2} {}", amount, currency)
        }))))
    }

    async fn handle_record_expense(
        &self,
        engine: &BusinessEngine,
        args: &Value,
    ) -> Result<ToolResult> {
        let business_id = match args.get("business_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required parameter: business_id")),
        };
        let amount = match args.get("amount").and_then(|v| v.as_f64()) {
            Some(a) => a,
            None => return Ok(ToolResult::error("Missing required parameter: amount")),
        };
        let category = args
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("general");
        let currency = args
            .get("currency")
            .and_then(|v| v.as_str())
            .unwrap_or("EUR");
        let description = args.get("description").and_then(|v| v.as_str());

        match engine
            .record_expense(business_id, amount, currency, category, description)
            .await
        {
            Ok(tx) => Ok(ToolResult::success(json_output(json!({
                "transaction_id": tx.id,
                "message": format!("Expense recorded: {:.2} {} ({})", amount, currency, category)
            })))),
            Err(e) => Ok(ToolResult::error(format!("{e}"))),
        }
    }

    async fn handle_revenue(
        &self,
        engine: &BusinessEngine,
        args: &Value,
    ) -> Result<ToolResult> {
        let business_id = match args.get("business_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required parameter: business_id")),
        };

        let summary = engine.get_revenue_summary(business_id).await?;
        Ok(ToolResult::success(json_output(json!({
            "income": summary.income,
            "expenses": summary.expenses,
            "refunds": summary.refunds,
            "profit": summary.profit,
            "tax_collected": summary.tax_collected,
            "budget_total": summary.budget_total,
            "budget_remaining": summary.budget_remaining,
        }))))
    }

    async fn handle_review(
        &self,
        engine: &BusinessEngine,
        args: &Value,
    ) -> Result<ToolResult> {
        let business_id = match args.get("business_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required parameter: business_id")),
        };

        let report = engine.status_report(business_id).await?;
        let revenue = engine.get_revenue_summary(business_id).await?;
        let strategies = engine.db().list_strategies(business_id).await?;
        let insights = engine.db().list_insights(business_id).await?;

        Ok(ToolResult::success(json_output(json!({
            "report": report,
            "revenue": {
                "income": revenue.income,
                "expenses": revenue.expenses,
                "profit": revenue.profit,
            },
            "strategies_count": strategies.len(),
            "insights_count": insights.len(),
            "recent_insights": insights.iter().take(5).map(|i| json!({
                "topic": i.topic,
                "type": i.insight_type,
                "confidence": i.confidence,
            })).collect::<Vec<_>>(),
        }))))
    }

    async fn handle_pivot(
        &self,
        engine: &BusinessEngine,
        args: &Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let business_id = match args.get("business_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required parameter: business_id")),
        };
        let hypothesis = match args.get("hypothesis").and_then(|v| v.as_str()) {
            Some(h) => h,
            None => return Ok(ToolResult::error("Missing required parameter: hypothesis")),
        };
        let from_strategy_id = args.get("from_strategy_id").and_then(|v| v.as_str());
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Pivot strategy");

        // Mark old strategy as pivoted
        if let Some(old_id) = from_strategy_id {
            engine
                .db()
                .update_strategy_status(old_id, StrategyStatus::Pivoted)
                .await?;
        }

        // Create new strategy
        let new_strategy = engine
            .add_strategy(business_id, name, hypothesis, None)
            .await?;

        let biz = engine
            .db()
            .load_business(business_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Business not found"))?;

        if biz.autonomy_level == BusinessAutonomy::Semi {
            Ok(ToolResult::success(json_output(json!({
                "new_strategy_id": new_strategy.id,
                "status": "proposed",
                "message": format!(
                    "Pivot proposed: '{}'. Semi-autonomous mode — present to user for approval.",
                    hypothesis
                )
            }))))
        } else {
            engine
                .db()
                .update_strategy_status(&new_strategy.id, StrategyStatus::Active)
                .await?;
            Ok(ToolResult::success(json_output(json!({
                "new_strategy_id": new_strategy.id,
                "status": "active",
                "message": format!("Strategy pivoted to: '{}'", hypothesis)
            }))))
        }
    }

    async fn handle_pause(
        &self,
        engine: &BusinessEngine,
        args: &Value,
    ) -> Result<ToolResult> {
        let business_id = match args.get("business_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required parameter: business_id")),
        };

        match engine.pause(business_id).await {
            Ok(()) => Ok(ToolResult::success(json_output(json!({
                "message": "Business paused. OODA reviews will stop until resumed."
            })))),
            Err(e) => Ok(ToolResult::error(format!("{e}"))),
        }
    }

    async fn handle_close(
        &self,
        engine: &BusinessEngine,
        args: &Value,
    ) -> Result<ToolResult> {
        let business_id = match args.get("business_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required parameter: business_id")),
        };

        // Get final summary before closing
        let summary = engine.get_revenue_summary(business_id).await?;
        engine.close(business_id).await?;

        Ok(ToolResult::success(json_output(json!({
            "message": "Business closed permanently.",
            "final_revenue": {
                "income": summary.income,
                "expenses": summary.expenses,
                "profit": summary.profit,
            }
        }))))
    }
}
