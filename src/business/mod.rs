//! Business Autopilot — types and data structures.
//!
//! Core domain types for autonomous business management:
//! businesses, strategies, products, transactions, orders, insights.

pub mod db;
pub mod engine;

use serde::{Deserialize, Serialize};

// ── Status enums ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BusinessStatus {
    Planning,
    Active,
    Paused,
    Closed,
}

impl BusinessStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planning => "planning",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Closed => "closed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "paused" => Self::Paused,
            "closed" => Self::Closed,
            _ => Self::Planning,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Closed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BusinessAutonomy {
    Semi,
    Budget,
    Full,
}

impl BusinessAutonomy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Semi => "semi",
            Self::Budget => "budget",
            Self::Full => "full",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "budget" => Self::Budget,
            "full" => Self::Full,
            _ => Self::Semi,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyStatus {
    Proposed,
    Approved,
    Active,
    Pivoted,
    Abandoned,
}

impl StrategyStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Approved => "approved",
            Self::Active => "active",
            Self::Pivoted => "pivoted",
            Self::Abandoned => "abandoned",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "approved" => Self::Approved,
            "active" => Self::Active,
            "pivoted" => Self::Pivoted,
            "abandoned" => Self::Abandoned,
            _ => Self::Proposed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductStatus {
    Draft,
    Active,
    Discontinued,
}

impl ProductStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Discontinued => "discontinued",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "discontinued" => Self::Discontinued,
            _ => Self::Draft,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxType {
    Income,
    Expense,
    Refund,
}

impl TxType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Income => "income",
            Self::Expense => "expense",
            Self::Refund => "refund",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "expense" => Self::Expense,
            "refund" => Self::Refund,
            _ => Self::Income,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Pending,
    Paid,
    Fulfilled,
    Refunded,
    Cancelled,
}

impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Paid => "paid",
            Self::Fulfilled => "fulfilled",
            Self::Refunded => "refunded",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "paid" => Self::Paid,
            "fulfilled" => Self::Fulfilled,
            "refunded" => Self::Refunded,
            "cancelled" => Self::Cancelled,
            _ => Self::Pending,
        }
    }
}

// ── Core structs ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Business {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: BusinessStatus,
    pub autonomy_level: BusinessAutonomy,
    pub budget_total: Option<f64>,
    pub budget_spent: f64,
    pub budget_currency: String,
    pub ooda_automation_id: Option<String>,
    pub ooda_interval: String,
    pub created_by: Option<String>,
    pub deliver_to: Option<String>,
    pub context: serde_json::Value,
    pub fiscal_config: Option<FiscalConfig>,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiscalConfig {
    pub country: String,
    pub vat_number: Option<String>,
    pub regime: String,
    pub default_tax_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub id: String,
    pub business_id: String,
    pub name: String,
    pub hypothesis: String,
    pub approach: Option<String>,
    pub status: StrategyStatus,
    pub metrics: Option<serde_json::Value>,
    pub results: Option<serde_json::Value>,
    pub approved_at: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: String,
    pub business_id: String,
    pub name: String,
    pub description: Option<String>,
    pub product_type: String,
    pub price: f64,
    pub currency: String,
    pub status: ProductStatus,
    pub metadata: Option<serde_json::Value>,
    pub units_sold: i64,
    pub revenue_total: f64,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub business_id: String,
    pub product_id: Option<String>,
    pub order_id: Option<String>,
    pub tx_type: TxType,
    pub amount: f64,
    pub currency: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub source: Option<String>,
    pub tax_amount: Option<f64>,
    pub tax_rate: Option<f64>,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub business_id: String,
    pub product_id: String,
    pub customer_email: Option<String>,
    pub customer_name: Option<String>,
    pub customer_country: Option<String>,
    pub amount: f64,
    pub tax_amount: f64,
    pub currency: String,
    pub payment_provider: Option<String>,
    pub payment_ref: Option<String>,
    pub status: OrderStatus,
    pub invoice_ref: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketInsight {
    pub id: String,
    pub business_id: String,
    pub topic: String,
    pub insight_type: String,
    pub content: String,
    pub confidence: Option<f64>,
    pub source: Option<String>,
    pub created_at: String,
}

// ── Revenue summary ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueSummary {
    pub income: f64,
    pub expenses: f64,
    pub refunds: f64,
    pub profit: f64,
    pub tax_collected: f64,
    pub budget_total: Option<f64>,
    pub budget_remaining: Option<f64>,
}
