# Business

## Purpose

This subsystem owns the current Business Autopilot core: business entities, strategy/product/transaction/order records, budget enforcement, OODA review prompt generation, and the shared business engine used by tools and web/UI surfaces.

## Primary Code

- `src/business/mod.rs`
- `src/business/db.rs`
- `src/business/engine.rs`
- `src/tools/business.rs`

## Domain Model

The current business domain already persists:

- businesses
- strategies
- products
- transactions
- orders
- market insights
- revenue summaries

Status enums exist for business lifecycle, strategy lifecycle, product lifecycle, transactions, and orders.

## Engine Behavior

`BusinessEngine` currently provides:

- launch a business
- pause/resume/close it
- build the OODA review prompt
- link an automation to the business
- enforce budget checks
- record sales
- record expenses
- expose DB-backed business state to higher layers

The engine is orchestration logic around the business tables, not a full payment/accounting stack.

## OODA Integration

The business engine can build a review prompt intended for scheduled OODA cycles. That prompt expects the agent to use the `business` tool and any available MCP integrations to inspect state, analyze performance, and propose or execute next actions depending on autonomy level.

Current autonomy levels:

- `semi`
- `budget`
- `full`

## Persistence

Business persistence comes from the business migration set in SQLite, not from a separate store.

## Current Limits

This area is explicitly only the core foundation compared to the roadmap vision.

What exists:

- business domain tables
- lifecycle state
- budget checks
- revenue/expense recording
- OODA prompt generation
- web/runtime wiring

What still remains roadmap-heavy:

- payment execution
- advanced accounting
- marketing execution
- broader business autopilot flows

## Change Checklist

Update this document when you change:

- business domain schema
- autonomy/budget rules
- OODA prompt behavior
- scope of the business engine versus roadmap aspirations
