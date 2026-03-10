//! Business engine — orchestrates business lifecycle and OODA review loop.
//!
//! The engine:
//! 1. Launches businesses with configurable autonomy
//! 2. Creates OODA review automations for periodic strategy assessment
//! 3. Manages lifecycle (pause/resume/close)
//! 4. Enforces budget constraints
//! 5. Provides revenue summaries

use anyhow::{bail, Result};
use chrono::Utc;

use crate::storage::Database;

use super::{
    Business, BusinessAutonomy, BusinessStatus, FiscalConfig, MarketInsight, Product,
    ProductStatus, RevenueSummary, Strategy, StrategyStatus, Transaction, TxType,
};

pub struct BusinessEngine {
    db: Database,
}

impl BusinessEngine {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Launch a new business venture.
    pub async fn launch(
        &self,
        name: &str,
        description: Option<&str>,
        autonomy: BusinessAutonomy,
        budget: Option<f64>,
        currency: &str,
        ooda_interval: &str,
        deliver_to: Option<&str>,
        created_by: Option<&str>,
        fiscal_config: Option<FiscalConfig>,
    ) -> Result<Business> {
        let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let now = Utc::now().to_rfc3339();

        let biz = Business {
            id: id.clone(),
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            status: BusinessStatus::Active,
            autonomy_level: autonomy,
            budget_total: budget,
            budget_spent: 0.0,
            budget_currency: currency.to_string(),
            ooda_automation_id: None,
            ooda_interval: ooda_interval.to_string(),
            created_by: created_by.map(|s| s.to_string()),
            deliver_to: deliver_to.map(|s| s.to_string()),
            context: serde_json::json!({}),
            fiscal_config,
            created_at: now,
            updated_at: None,
            closed_at: None,
        };

        self.db.insert_business(&biz).await?;

        tracing::info!(
            business_id = %id,
            name = %name,
            autonomy = %autonomy.as_str(),
            "Business launched"
        );

        Ok(biz)
    }

    /// Create an OODA review automation for a business.
    /// Returns the automation ID so it can be linked to the business.
    pub fn build_ooda_prompt(&self, biz: &Business) -> String {
        let budget_info = match (biz.budget_total, biz.budget_spent) {
            (Some(total), spent) => format!(
                "Budget: {:.2} {} (spent: {:.2}, remaining: {:.2})",
                total,
                biz.budget_currency,
                spent,
                total - spent
            ),
            _ => "Budget: unlimited".to_string(),
        };

        format!(
            r#"BUSINESS REVIEW for "{name}" (id: {id})

You are performing an OODA review cycle for this business.

1. OBSERVE: Use business(action="status", business_id="{id}") to get current state
2. ORIENT: Use business(action="revenue", business_id="{id}") to check financial performance
3. DECIDE: Analyze if the current strategy is working. Are KPIs on target?
4. ACT: If a pivot is needed, use business(action="pivot"). If new research is needed, use business(action="research").
5. REPORT: Summarize findings and send to the business owner.

Autonomy level: {autonomy}
{budget}

Rules based on autonomy:
- semi: Propose significant actions to the user for approval before executing
- budget: Execute freely within budget, but propose anything exceeding remaining budget
- full: Execute all actions autonomously

If MCP tools are available (e.g., for marketing, payments, analytics), use them to gather data and execute actions."#,
            name = biz.name,
            id = biz.id,
            autonomy = biz.autonomy_level.as_str(),
            budget = budget_info,
        )
    }

    /// Link an OODA automation to a business.
    pub async fn set_ooda_automation(
        &self,
        business_id: &str,
        automation_id: &str,
    ) -> Result<()> {
        self.db
            .set_business_ooda_automation(business_id, automation_id)
            .await
    }

    /// Pause a business (stops OODA reviews).
    pub async fn pause(&self, id: &str) -> Result<()> {
        let biz = self
            .db
            .load_business(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Business not found: {id}"))?;

        if biz.status.is_terminal() {
            bail!("Cannot pause a closed business");
        }

        self.db
            .update_business_status(id, BusinessStatus::Paused)
            .await?;

        tracing::info!(business_id = %id, "Business paused");
        Ok(())
    }

    /// Resume a paused business.
    pub async fn resume(&self, id: &str) -> Result<()> {
        let biz = self
            .db
            .load_business(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Business not found: {id}"))?;

        if biz.status != BusinessStatus::Paused {
            bail!("Business is not paused (status: {})", biz.status.as_str());
        }

        self.db
            .update_business_status(id, BusinessStatus::Active)
            .await?;

        tracing::info!(business_id = %id, "Business resumed");
        Ok(())
    }

    /// Close a business permanently.
    pub async fn close(&self, id: &str) -> Result<()> {
        self.db
            .update_business_status(id, BusinessStatus::Closed)
            .await?;

        tracing::info!(business_id = %id, "Business closed");
        Ok(())
    }

    /// Check if a proposed expense fits within budget.
    pub async fn check_budget(&self, business_id: &str, amount: f64) -> Result<bool> {
        let biz = self
            .db
            .load_business(business_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Business not found: {business_id}"))?;

        match biz.budget_total {
            Some(total) => {
                let remaining = total - biz.budget_spent;
                Ok(amount <= remaining)
            }
            None => Ok(true), // No budget limit
        }
    }

    /// Record a sale: creates transaction + updates product stats.
    pub async fn record_sale(
        &self,
        business_id: &str,
        amount: f64,
        currency: &str,
        product_id: Option<&str>,
        description: Option<&str>,
        tax_amount: Option<f64>,
        tax_rate: Option<f64>,
        source: Option<&str>,
    ) -> Result<Transaction> {
        let tx_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let now = Utc::now().to_rfc3339();

        let tx = Transaction {
            id: tx_id,
            business_id: business_id.to_string(),
            product_id: product_id.map(|s| s.to_string()),
            order_id: None,
            tx_type: TxType::Income,
            amount,
            currency: currency.to_string(),
            description: description.map(|s| s.to_string()),
            category: Some("sale".to_string()),
            source: source.map(|s| s.to_string()),
            tax_amount,
            tax_rate,
            recorded_at: now,
        };

        self.db.insert_transaction(&tx).await?;

        // Update product counters if linked
        if let Some(pid) = product_id {
            self.db.update_product_sales(pid, 1, amount).await?;
        }

        Ok(tx)
    }

    /// Record an expense with budget enforcement.
    pub async fn record_expense(
        &self,
        business_id: &str,
        amount: f64,
        currency: &str,
        category: &str,
        description: Option<&str>,
    ) -> Result<Transaction> {
        // Budget check
        if !self.check_budget(business_id, amount).await? {
            let biz = self.db.load_business(business_id).await?.unwrap();
            let remaining = biz.budget_total.unwrap_or(0.0) - biz.budget_spent;
            bail!(
                "Budget exceeded: expense {:.2} > remaining {:.2} {}",
                amount,
                remaining,
                biz.budget_currency
            );
        }

        let tx_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let now = Utc::now().to_rfc3339();

        let tx = Transaction {
            id: tx_id,
            business_id: business_id.to_string(),
            product_id: None,
            order_id: None,
            tx_type: TxType::Expense,
            amount,
            currency: currency.to_string(),
            description: description.map(|s| s.to_string()),
            category: Some(category.to_string()),
            source: None,
            tax_amount: None,
            tax_rate: None,
            recorded_at: now,
        };

        self.db.insert_transaction(&tx).await?;

        // Update budget spent
        self.db.update_budget_spent(business_id).await?;

        Ok(tx)
    }

    /// Add a strategy proposal.
    pub async fn add_strategy(
        &self,
        business_id: &str,
        name: &str,
        hypothesis: &str,
        approach: Option<&str>,
    ) -> Result<Strategy> {
        let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let now = Utc::now().to_rfc3339();

        let strategy = Strategy {
            id,
            business_id: business_id.to_string(),
            name: name.to_string(),
            hypothesis: hypothesis.to_string(),
            approach: approach.map(|s| s.to_string()),
            status: StrategyStatus::Proposed,
            metrics: None,
            results: None,
            approved_at: None,
            created_at: now,
            updated_at: None,
        };

        self.db.insert_strategy(&strategy).await?;
        Ok(strategy)
    }

    /// Create a product.
    pub async fn create_product(
        &self,
        business_id: &str,
        name: &str,
        description: Option<&str>,
        product_type: &str,
        price: f64,
        currency: &str,
    ) -> Result<Product> {
        let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let now = Utc::now().to_rfc3339();

        let product = Product {
            id,
            business_id: business_id.to_string(),
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            product_type: product_type.to_string(),
            price,
            currency: currency.to_string(),
            status: ProductStatus::Draft,
            metadata: None,
            units_sold: 0,
            revenue_total: 0.0,
            created_at: now,
            updated_at: None,
        };

        self.db.insert_product(&product).await?;
        Ok(product)
    }

    /// Record a market insight.
    pub async fn add_insight(
        &self,
        business_id: &str,
        topic: &str,
        insight_type: &str,
        content: &str,
        confidence: Option<f64>,
        source: Option<&str>,
    ) -> Result<MarketInsight> {
        let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let now = Utc::now().to_rfc3339();

        let insight = MarketInsight {
            id,
            business_id: business_id.to_string(),
            topic: topic.to_string(),
            insight_type: insight_type.to_string(),
            content: content.to_string(),
            confidence,
            source: source.map(|s| s.to_string()),
            created_at: now,
        };

        self.db.insert_insight(&insight).await?;
        Ok(insight)
    }

    /// Get revenue summary for a business.
    pub async fn get_revenue_summary(&self, business_id: &str) -> Result<RevenueSummary> {
        self.db.revenue_summary(business_id).await
    }

    /// Get a business status report (human-readable).
    pub async fn status_report(&self, business_id: &str) -> Result<String> {
        let biz = self
            .db
            .load_business(business_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Business not found: {business_id}"))?;

        let strategies = self.db.list_strategies(business_id).await?;
        let products = self.db.list_products(business_id).await?;
        let revenue = self.db.revenue_summary(business_id).await?;

        let mut report = format!(
            "**{}** ({})\nStatus: {} | Autonomy: {}\n",
            biz.name,
            biz.id,
            biz.status.as_str(),
            biz.autonomy_level.as_str(),
        );

        // Budget
        if let Some(total) = biz.budget_total {
            report.push_str(&format!(
                "Budget: {:.2}/{:.2} {} (remaining: {:.2})\n",
                biz.budget_spent,
                total,
                biz.budget_currency,
                total - biz.budget_spent,
            ));
        }

        // Revenue
        report.push_str(&format!(
            "\nRevenue: {:.2} | Expenses: {:.2} | Profit: {:.2}\n",
            revenue.income, revenue.expenses, revenue.profit,
        ));

        // Strategies
        if !strategies.is_empty() {
            report.push_str(&format!("\nStrategies ({}):\n", strategies.len()));
            for s in &strategies {
                report.push_str(&format!(
                    "  - {} [{}]: {}\n",
                    s.name,
                    s.status.as_str(),
                    s.hypothesis
                ));
            }
        }

        // Products
        if !products.is_empty() {
            report.push_str(&format!("\nProducts ({}):\n", products.len()));
            for p in &products {
                report.push_str(&format!(
                    "  - {} ({:.2} {}) [{}] — {} sold, {:.2} revenue\n",
                    p.name,
                    p.price,
                    p.currency,
                    p.status.as_str(),
                    p.units_sold,
                    p.revenue_total,
                ));
            }
        }

        Ok(report)
    }
}
