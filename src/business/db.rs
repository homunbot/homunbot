//! Database operations for the business module.

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::FromRow;

use crate::storage::Database;

use super::{
    Business, BusinessAutonomy, BusinessStatus, FiscalConfig, MarketInsight, Order, OrderStatus,
    Product, ProductStatus, RevenueSummary, Strategy, StrategyStatus, Transaction, TxType,
};

// ── Row types for sqlx ───────────────────────────────────────────────

#[derive(FromRow)]
struct BusinessRow {
    id: String,
    name: String,
    description: Option<String>,
    status: String,
    autonomy_level: String,
    budget_total: Option<f64>,
    budget_spent: f64,
    budget_currency: String,
    ooda_automation_id: Option<String>,
    ooda_interval: String,
    created_by: Option<String>,
    deliver_to: Option<String>,
    context_json: String,
    fiscal_config_json: Option<String>,
    created_at: String,
    updated_at: Option<String>,
    closed_at: Option<String>,
}

impl BusinessRow {
    fn into_business(self) -> Business {
        let context = serde_json::from_str(&self.context_json)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        let fiscal_config = self
            .fiscal_config_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        Business {
            id: self.id,
            name: self.name,
            description: self.description,
            status: BusinessStatus::from_str(&self.status),
            autonomy_level: BusinessAutonomy::from_str(&self.autonomy_level),
            budget_total: self.budget_total,
            budget_spent: self.budget_spent,
            budget_currency: self.budget_currency,
            ooda_automation_id: self.ooda_automation_id,
            ooda_interval: self.ooda_interval,
            created_by: self.created_by,
            deliver_to: self.deliver_to,
            context,
            fiscal_config,
            created_at: self.created_at,
            updated_at: self.updated_at,
            closed_at: self.closed_at,
        }
    }
}

#[derive(FromRow)]
struct StrategyRow {
    id: String,
    business_id: String,
    name: String,
    hypothesis: String,
    approach: Option<String>,
    status: String,
    metrics_json: Option<String>,
    results_json: Option<String>,
    approved_at: Option<String>,
    created_at: String,
    updated_at: Option<String>,
}

impl StrategyRow {
    fn into_strategy(self) -> Strategy {
        Strategy {
            id: self.id,
            business_id: self.business_id,
            name: self.name,
            hypothesis: self.hypothesis,
            approach: self.approach,
            status: StrategyStatus::from_str(&self.status),
            metrics: self
                .metrics_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok()),
            results: self
                .results_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok()),
            approved_at: self.approved_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(FromRow)]
struct ProductRow {
    id: String,
    business_id: String,
    name: String,
    description: Option<String>,
    product_type: String,
    price: f64,
    currency: String,
    status: String,
    metadata_json: Option<String>,
    units_sold: i64,
    revenue_total: f64,
    created_at: String,
    updated_at: Option<String>,
}

impl ProductRow {
    fn into_product(self) -> Product {
        Product {
            id: self.id,
            business_id: self.business_id,
            name: self.name,
            description: self.description,
            product_type: self.product_type,
            price: self.price,
            currency: self.currency,
            status: ProductStatus::from_str(&self.status),
            metadata: self
                .metadata_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok()),
            units_sold: self.units_sold,
            revenue_total: self.revenue_total,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(FromRow)]
struct TransactionRow {
    id: String,
    business_id: String,
    product_id: Option<String>,
    order_id: Option<String>,
    tx_type: String,
    amount: f64,
    currency: String,
    description: Option<String>,
    category: Option<String>,
    source: Option<String>,
    tax_amount: Option<f64>,
    tax_rate: Option<f64>,
    recorded_at: String,
}

impl TransactionRow {
    fn into_transaction(self) -> Transaction {
        Transaction {
            id: self.id,
            business_id: self.business_id,
            product_id: self.product_id,
            order_id: self.order_id,
            tx_type: TxType::from_str(&self.tx_type),
            amount: self.amount,
            currency: self.currency,
            description: self.description,
            category: self.category,
            source: self.source,
            tax_amount: self.tax_amount,
            tax_rate: self.tax_rate,
            recorded_at: self.recorded_at,
        }
    }
}

#[derive(FromRow)]
struct OrderRow {
    id: String,
    business_id: String,
    product_id: String,
    customer_email: Option<String>,
    customer_name: Option<String>,
    customer_country: Option<String>,
    amount: f64,
    tax_amount: f64,
    currency: String,
    payment_provider: Option<String>,
    payment_ref: Option<String>,
    status: String,
    invoice_ref: Option<String>,
    notes: Option<String>,
    created_at: String,
    completed_at: Option<String>,
}

impl OrderRow {
    fn into_order(self) -> Order {
        Order {
            id: self.id,
            business_id: self.business_id,
            product_id: self.product_id,
            customer_email: self.customer_email,
            customer_name: self.customer_name,
            customer_country: self.customer_country,
            amount: self.amount,
            tax_amount: self.tax_amount,
            currency: self.currency,
            payment_provider: self.payment_provider,
            payment_ref: self.payment_ref,
            status: OrderStatus::from_str(&self.status),
            invoice_ref: self.invoice_ref,
            notes: self.notes,
            created_at: self.created_at,
            completed_at: self.completed_at,
        }
    }
}

#[derive(FromRow)]
struct InsightRow {
    id: String,
    business_id: String,
    topic: String,
    insight_type: String,
    content: String,
    confidence: Option<f64>,
    source: Option<String>,
    created_at: String,
}

impl InsightRow {
    fn into_insight(self) -> MarketInsight {
        MarketInsight {
            id: self.id,
            business_id: self.business_id,
            topic: self.topic,
            insight_type: self.insight_type,
            content: self.content,
            confidence: self.confidence,
            source: self.source,
            created_at: self.created_at,
        }
    }
}

impl Database {
    // ── Business CRUD ────────────────────────────────────────────────

    pub async fn insert_business(&self, biz: &Business) -> Result<()> {
        let context_json = serde_json::to_string(&biz.context)?;
        let fiscal_json = biz
            .fiscal_config
            .as_ref()
            .map(|f| serde_json::to_string(f))
            .transpose()?;

        sqlx::query(
            "INSERT INTO businesses (id, name, description, status, autonomy_level,
             budget_total, budget_spent, budget_currency, ooda_automation_id, ooda_interval,
             created_by, deliver_to, context_json, fiscal_config_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&biz.id)
        .bind(&biz.name)
        .bind(&biz.description)
        .bind(biz.status.as_str())
        .bind(biz.autonomy_level.as_str())
        .bind(biz.budget_total)
        .bind(biz.budget_spent)
        .bind(&biz.budget_currency)
        .bind(&biz.ooda_automation_id)
        .bind(&biz.ooda_interval)
        .bind(&biz.created_by)
        .bind(&biz.deliver_to)
        .bind(&context_json)
        .bind(&fiscal_json)
        .bind(&biz.created_at)
        .execute(self.pool())
        .await
        .with_context(|| format!("Failed to insert business {}", biz.id))?;

        Ok(())
    }

    pub async fn load_business(&self, id: &str) -> Result<Option<Business>> {
        let row = sqlx::query_as::<_, BusinessRow>("SELECT * FROM businesses WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await
            .with_context(|| format!("Failed to load business {id}"))?;

        Ok(row.map(|r| r.into_business()))
    }

    pub async fn list_businesses(&self, status_filter: Option<&str>) -> Result<Vec<Business>> {
        let rows = if let Some(status) = status_filter {
            sqlx::query_as::<_, BusinessRow>(
                "SELECT * FROM businesses WHERE status = ? ORDER BY created_at DESC",
            )
            .bind(status)
            .fetch_all(self.pool())
            .await?
        } else {
            sqlx::query_as::<_, BusinessRow>("SELECT * FROM businesses ORDER BY created_at DESC")
                .fetch_all(self.pool())
                .await?
        };

        Ok(rows.into_iter().map(|r| r.into_business()).collect())
    }

    pub async fn update_business_status(&self, id: &str, status: BusinessStatus) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let closed_at = if status.is_terminal() {
            Some(now.clone())
        } else {
            None
        };

        sqlx::query(
            "UPDATE businesses SET status = ?, updated_at = ?, closed_at = COALESCE(?, closed_at) WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(&now)
        .bind(closed_at)
        .bind(id)
        .execute(self.pool())
        .await
        .with_context(|| format!("Failed to update business status {id}"))?;

        Ok(())
    }

    pub async fn set_business_ooda_automation(&self, id: &str, automation_id: &str) -> Result<()> {
        sqlx::query("UPDATE businesses SET ooda_automation_id = ? WHERE id = ?")
            .bind(automation_id)
            .bind(id)
            .execute(self.pool())
            .await
            .with_context(|| format!("Failed to set OODA automation for business {id}"))?;
        Ok(())
    }

    pub async fn update_budget_spent(&self, business_id: &str) -> Result<f64> {
        let spent: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0.0) FROM transactions
             WHERE business_id = ? AND tx_type = 'expense'",
        )
        .bind(business_id)
        .fetch_one(self.pool())
        .await
        .unwrap_or(0.0);

        sqlx::query("UPDATE businesses SET budget_spent = ?, updated_at = ? WHERE id = ?")
            .bind(spent)
            .bind(Utc::now().to_rfc3339())
            .bind(business_id)
            .execute(self.pool())
            .await?;

        Ok(spent)
    }

    // ── Strategy CRUD ────────────────────────────────────────────────

    pub async fn insert_strategy(&self, s: &Strategy) -> Result<()> {
        let metrics_json = s
            .metrics
            .as_ref()
            .map(|m| serde_json::to_string(m))
            .transpose()?;
        let results_json = s
            .results
            .as_ref()
            .map(|r| serde_json::to_string(r))
            .transpose()?;

        sqlx::query(
            "INSERT INTO business_strategies (id, business_id, name, hypothesis, approach, status, metrics_json, results_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&s.id)
        .bind(&s.business_id)
        .bind(&s.name)
        .bind(&s.hypothesis)
        .bind(&s.approach)
        .bind(s.status.as_str())
        .bind(&metrics_json)
        .bind(&results_json)
        .bind(&s.created_at)
        .execute(self.pool())
        .await
        .with_context(|| format!("Failed to insert strategy {}", s.id))?;

        Ok(())
    }

    pub async fn list_strategies(&self, business_id: &str) -> Result<Vec<Strategy>> {
        let rows = sqlx::query_as::<_, StrategyRow>(
            "SELECT * FROM business_strategies WHERE business_id = ? ORDER BY created_at DESC",
        )
        .bind(business_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_strategy()).collect())
    }

    pub async fn update_strategy_status(&self, id: &str, status: StrategyStatus) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let approved_at = if status == StrategyStatus::Approved {
            Some(now.clone())
        } else {
            None
        };

        sqlx::query(
            "UPDATE business_strategies SET status = ?, updated_at = ?, approved_at = COALESCE(?, approved_at) WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(&now)
        .bind(approved_at)
        .bind(id)
        .execute(self.pool())
        .await?;

        Ok(())
    }

    // ── Product CRUD ─────────────────────────────────────────────────

    pub async fn insert_product(&self, p: &Product) -> Result<()> {
        let metadata_json = p
            .metadata
            .as_ref()
            .map(|m| serde_json::to_string(m))
            .transpose()?;

        sqlx::query(
            "INSERT INTO products (id, business_id, name, description, product_type, price, currency, status, metadata_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&p.id)
        .bind(&p.business_id)
        .bind(&p.name)
        .bind(&p.description)
        .bind(&p.product_type)
        .bind(p.price)
        .bind(&p.currency)
        .bind(p.status.as_str())
        .bind(&metadata_json)
        .bind(&p.created_at)
        .execute(self.pool())
        .await
        .with_context(|| format!("Failed to insert product {}", p.id))?;

        Ok(())
    }

    pub async fn list_products(&self, business_id: &str) -> Result<Vec<Product>> {
        let rows = sqlx::query_as::<_, ProductRow>(
            "SELECT * FROM products WHERE business_id = ? ORDER BY created_at DESC",
        )
        .bind(business_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_product()).collect())
    }

    pub async fn update_product_sales(
        &self,
        id: &str,
        units_delta: i64,
        revenue_delta: f64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE products SET units_sold = units_sold + ?, revenue_total = revenue_total + ?, updated_at = ? WHERE id = ?",
        )
        .bind(units_delta)
        .bind(revenue_delta)
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(self.pool())
        .await?;

        Ok(())
    }

    // ── Transaction CRUD ─────────────────────────────────────────────

    pub async fn insert_transaction(&self, tx: &Transaction) -> Result<()> {
        sqlx::query(
            "INSERT INTO transactions (id, business_id, product_id, order_id, tx_type, amount, currency, description, category, source, tax_amount, tax_rate, recorded_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&tx.id)
        .bind(&tx.business_id)
        .bind(&tx.product_id)
        .bind(&tx.order_id)
        .bind(tx.tx_type.as_str())
        .bind(tx.amount)
        .bind(&tx.currency)
        .bind(&tx.description)
        .bind(&tx.category)
        .bind(&tx.source)
        .bind(tx.tax_amount)
        .bind(tx.tax_rate)
        .bind(&tx.recorded_at)
        .execute(self.pool())
        .await
        .with_context(|| format!("Failed to insert transaction {}", tx.id))?;

        Ok(())
    }

    pub async fn list_transactions(&self, business_id: &str) -> Result<Vec<Transaction>> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            "SELECT * FROM transactions WHERE business_id = ? ORDER BY recorded_at DESC",
        )
        .bind(business_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_transaction()).collect())
    }

    pub async fn revenue_summary(&self, business_id: &str) -> Result<RevenueSummary> {
        let income: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0.0) FROM transactions WHERE business_id = ? AND tx_type = 'income'",
        )
        .bind(business_id)
        .fetch_one(self.pool())
        .await
        .unwrap_or(0.0);

        let expenses: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0.0) FROM transactions WHERE business_id = ? AND tx_type = 'expense'",
        )
        .bind(business_id)
        .fetch_one(self.pool())
        .await
        .unwrap_or(0.0);

        let refunds: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0.0) FROM transactions WHERE business_id = ? AND tx_type = 'refund'",
        )
        .bind(business_id)
        .fetch_one(self.pool())
        .await
        .unwrap_or(0.0);

        let tax_collected: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(tax_amount), 0.0) FROM transactions WHERE business_id = ? AND tx_type = 'income'",
        )
        .bind(business_id)
        .fetch_one(self.pool())
        .await
        .unwrap_or(0.0);

        // Get budget info
        let biz = self.load_business(business_id).await?;
        let budget_total = biz.as_ref().and_then(|b| b.budget_total);
        let budget_remaining = budget_total.map(|total| {
            let spent = biz.as_ref().map(|b| b.budget_spent).unwrap_or(0.0);
            total - spent
        });

        Ok(RevenueSummary {
            income,
            expenses,
            refunds,
            profit: income - expenses - refunds,
            tax_collected,
            budget_total,
            budget_remaining,
        })
    }

    // ── Order CRUD ───────────────────────────────────────────────────

    pub async fn insert_order(&self, order: &Order) -> Result<()> {
        sqlx::query(
            "INSERT INTO orders (id, business_id, product_id, customer_email, customer_name, customer_country,
             amount, tax_amount, currency, payment_provider, payment_ref, status, invoice_ref, notes, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&order.id)
        .bind(&order.business_id)
        .bind(&order.product_id)
        .bind(&order.customer_email)
        .bind(&order.customer_name)
        .bind(&order.customer_country)
        .bind(order.amount)
        .bind(order.tax_amount)
        .bind(&order.currency)
        .bind(&order.payment_provider)
        .bind(&order.payment_ref)
        .bind(order.status.as_str())
        .bind(&order.invoice_ref)
        .bind(&order.notes)
        .bind(&order.created_at)
        .execute(self.pool())
        .await
        .with_context(|| format!("Failed to insert order {}", order.id))?;

        Ok(())
    }

    pub async fn list_orders(&self, business_id: &str) -> Result<Vec<Order>> {
        let rows = sqlx::query_as::<_, OrderRow>(
            "SELECT * FROM orders WHERE business_id = ? ORDER BY created_at DESC",
        )
        .bind(business_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_order()).collect())
    }

    pub async fn update_order_status(&self, id: &str, status: OrderStatus) -> Result<()> {
        let completed_at = if matches!(status, OrderStatus::Paid | OrderStatus::Fulfilled) {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };

        sqlx::query(
            "UPDATE orders SET status = ?, completed_at = COALESCE(?, completed_at) WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(completed_at)
        .bind(id)
        .execute(self.pool())
        .await?;

        Ok(())
    }

    // ── Market Insight CRUD ──────────────────────────────────────────

    pub async fn insert_insight(&self, insight: &MarketInsight) -> Result<()> {
        sqlx::query(
            "INSERT INTO market_insights (id, business_id, topic, insight_type, content, confidence, source, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&insight.id)
        .bind(&insight.business_id)
        .bind(&insight.topic)
        .bind(&insight.insight_type)
        .bind(&insight.content)
        .bind(insight.confidence)
        .bind(&insight.source)
        .bind(&insight.created_at)
        .execute(self.pool())
        .await
        .with_context(|| format!("Failed to insert insight {}", insight.id))?;

        Ok(())
    }

    pub async fn list_insights(&self, business_id: &str) -> Result<Vec<MarketInsight>> {
        let rows = sqlx::query_as::<_, InsightRow>(
            "SELECT * FROM market_insights WHERE business_id = ? ORDER BY created_at DESC",
        )
        .bind(business_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_insight()).collect())
    }
}
