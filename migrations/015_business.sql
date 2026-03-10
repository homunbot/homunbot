-- Business Autopilot tables

-- businesses: ogni business venture
CREATE TABLE IF NOT EXISTS businesses (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'planning',
    autonomy_level TEXT NOT NULL DEFAULT 'semi',
    budget_total REAL,
    budget_spent REAL NOT NULL DEFAULT 0.0,
    budget_currency TEXT NOT NULL DEFAULT 'EUR',
    ooda_automation_id TEXT,
    ooda_interval TEXT NOT NULL DEFAULT 'every:86400',
    created_by TEXT,
    deliver_to TEXT,
    context_json TEXT NOT NULL DEFAULT '{}',
    fiscal_config_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT,
    closed_at TEXT
);

-- business_strategies: ipotesi e strategie
CREATE TABLE IF NOT EXISTS business_strategies (
    id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    hypothesis TEXT NOT NULL,
    approach TEXT,
    status TEXT NOT NULL DEFAULT 'proposed',
    metrics_json TEXT,
    results_json TEXT,
    approved_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT
);

-- products: prodotti/servizi in vendita
CREATE TABLE IF NOT EXISTS products (
    id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    product_type TEXT NOT NULL DEFAULT 'digital',
    price REAL NOT NULL DEFAULT 0.0,
    currency TEXT NOT NULL DEFAULT 'EUR',
    status TEXT NOT NULL DEFAULT 'draft',
    metadata_json TEXT,
    units_sold INTEGER NOT NULL DEFAULT 0,
    revenue_total REAL NOT NULL DEFAULT 0.0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT
);

-- transactions: ledger finanziario
CREATE TABLE IF NOT EXISTS transactions (
    id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    product_id TEXT,
    order_id TEXT,
    tx_type TEXT NOT NULL,
    amount REAL NOT NULL,
    currency TEXT NOT NULL DEFAULT 'EUR',
    description TEXT,
    category TEXT,
    source TEXT,
    tax_amount REAL,
    tax_rate REAL,
    recorded_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- orders: acquisti clienti
CREATE TABLE IF NOT EXISTS orders (
    id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    product_id TEXT NOT NULL,
    customer_email TEXT,
    customer_name TEXT,
    customer_country TEXT,
    amount REAL NOT NULL,
    tax_amount REAL NOT NULL DEFAULT 0.0,
    currency TEXT NOT NULL DEFAULT 'EUR',
    payment_provider TEXT,
    payment_ref TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    invoice_ref TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

-- market_insights: ricerche di mercato
CREATE TABLE IF NOT EXISTS market_insights (
    id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    topic TEXT NOT NULL,
    insight_type TEXT NOT NULL DEFAULT 'research',
    content TEXT NOT NULL,
    confidence REAL,
    source TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indici
CREATE INDEX IF NOT EXISTS idx_biz_status ON businesses(status);
CREATE INDEX IF NOT EXISTS idx_strat_biz ON business_strategies(business_id);
CREATE INDEX IF NOT EXISTS idx_prod_biz ON products(business_id);
CREATE INDEX IF NOT EXISTS idx_tx_biz ON transactions(business_id);
CREATE INDEX IF NOT EXISTS idx_tx_type ON transactions(tx_type);
CREATE INDEX IF NOT EXISTS idx_ord_biz ON orders(business_id);
CREATE INDEX IF NOT EXISTS idx_insight_biz ON market_insights(business_id);
