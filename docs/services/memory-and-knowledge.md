# Memory And Knowledge

## Purpose

This subsystem owns two related but separate knowledge layers:

- personal memory from conversations
- RAG knowledge base built from documents and synced resources

## Primary Code

- `src/agent/memory.rs`
- `src/agent/memory_search.rs`
- `src/rag/engine.rs`
- `src/rag/chunker.rs`
- `src/rag/parsers.rs`
- `src/rag/watcher.rs`
- `src/rag/cloud.rs`
- `src/rag/sensitive.rs`

## Memory Layer

The memory subsystem is conversation-derived and user-centric.

`MemoryConsolidator` does all of the following:

- watches session size against the memory window
- consolidates old session messages through an LLM call
- writes long-term facts into `MEMORY.md`
- appends event summaries into `HISTORY.md`
- learns instructions into `brain/INSTRUCTIONS.md`
- stores detected secrets in encrypted storage instead of plain memory
- produces memory chunks that can be embedded and indexed

This means memory is not just raw retrieval; it is a classification and condensation pipeline.

## Memory Search

`MemorySearcher` is hybrid retrieval over memory chunks:

- vector search
- SQLite FTS5
- reciprocal rank fusion
- temporal decay

This is the retrieval layer injected into the agent prompt for relevant past context.

## Knowledge / RAG Layer

The RAG subsystem is document-centric and source-attributed.

`RagEngine` handles:

- file ingestion
- deduplication by file hash
- chunking and parsing
- vector indexing
- FTS5 search
- source tracking
- sensitive-content redaction on retrieval
- reindexing/reingestion

Supported ingestion is gated by parser support and the `embeddings` build path.

## Watchers And Cloud Sync

Two supporting services extend the RAG engine:

- `RagWatcher`
  Watches configured directories and auto-reingests changed files.
- `CloudSync`
  Reads MCP resources, writes them into a sync directory, and ingests them into RAG.

The cloud sync path is the bridge between MCP resource servers and the local knowledge base.

## Persistence

Persistent state includes:

- memory chunks in SQLite
- RAG sources and chunks in SQLite
- FTS5 indexes in SQLite
- vector indexes on disk under `~/.homun/`
- memory markdown files under `~/.homun/brain/`

## Failure Modes And Limits

- local embeddings are optional; memory/RAG degrade when the embedding engine cannot initialize
- sensitive chunks are redacted at retrieval time
- document support is parser-dependent
- RAG and memory are related but intentionally not the same data model

## Change Checklist

Update this document when you change:

- consolidation output files
- memory search ranking
- RAG ingestion/search behavior
- watcher or MCP cloud sync behavior
