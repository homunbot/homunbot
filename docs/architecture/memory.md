# Architettura Memoria — Piano di Implementazione

> Decisioni prese, stack confermato, piano fase-per-fase
> Ultimo aggiornamento: 2026-02-21

---

## Decisione Finale: Stack Tecnico

### Embedding: `fastembed` (crate Rust)

| Aspetto | Scelta | Alternativa scartata |
|---------|--------|---------------------|
| **Crate** | `fastembed = "5"` | API esterne (come fa ZeroClaw) |
| **Modello default** | `ParaphraseMLMiniLML12V2Q` (quantizzato) | BGESmallENV15 (solo inglese) |
| **Dimensioni** | 384 dim, ~110MB modello | BGEM3 (1024 dim, ~1.2GB — troppo) |
| **Lingue** | 50+ (incluso italiano, inglese, tedesco, francese, spagnolo, ...) | Solo inglese |
| **Runtime** | ONNX locale, zero API | Richiede connessione internet |
| **Hardware** | Funziona su qualsiasi CPU (x86, ARM, RPi 4+) | GPU non necessaria |
| **Download** | Automatico al primo uso (~110MB una tantum) | — |

**Perche' ParaphraseMLMiniLML12V2Q**: il quantizzato e' piu' piccolo e veloce, stesse 384 dimensioni, multilingue. Perfetto per un assistente personale che puo' parlare in qualsiasi lingua. La variante Q usa int8 per i pesi (~50% piu' piccola).

**Alternative disponibili nel crate se serve cambiare**:
- `BGEM3` — 100+ lingue, 1024 dim, piu' preciso ma 3x piu' pesante
- `MultilingualE5Small` — buon compromesso, 384 dim
- `BGESmallENV15` — solo inglese ma leggerissimo (~33MB)

---

### Vector Search: Confronto Opzioni

Ricerca seria su cosa esiste **veramente embedded in Rust** (no server separati):

#### Opzione A: USearch — SCELTA ✓

| Aspetto | Dettaglio |
|---------|-----------|
| **Crate** | `usearch = "2"` |
| **Algoritmo** | HNSW (Hierarchical Navigable Small World) |
| **Scritto in** | C++ con binding Rust nativi (via cxx) |
| **Binary aggiuntivo** | ~320KB |
| **Performance** | 10x FAISS, SIMD ottimizzato (AVX2, NEON) |
| **Persistence** | `index.save("path")` / `Index::load("path")` — un singolo file |
| **Metriche** | Cosine, L2, InnerProduct |
| **Quantizzazione** | f32, f16, i8 nativa |
| **API** | Semplicissima: `new()`, `add()`, `search()`, `save()`, `load()` |
| **Thread-safe** | Si (Send + Sync) |
| **Maturo** | ~40k download/mese, 10+ binding linguistici |

```rust
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

let mut opts = IndexOptions::default();
opts.dimensions = 384;
opts.metric = MetricKind::Cos;
opts.quantization = ScalarKind::F32;

let index = Index::new(&opts)?;
index.reserve(10_000)?;
index.add(42, &embedding)?;         // key + vector
let results = index.search(&query, 5)?;  // top 5
index.save("~/.homun/memory.usearch")?;  // persist
```

**Perche' USearch**:
- HNSW e' lo standard industriale per ANN search — c'e' un motivo se Qdrant, Pinecone, Weaviate lo usano
- O(log N) per query vs O(N) del brute-force — cruciale quando la memoria cresce
- +320KB al binary (trascurabile vs i ~10MB di fastembed/ONNX)
- Persistence nativa su file — non serve SQLite per i vettori
- Filtered search per metadata (date range, source type)

#### Opzione B: hnswlib-rs — Alternativa valida

| Aspetto | Dettaglio |
|---------|-----------|
| **Crate** | `hnswlib-rs = "0.10"` |
| **Scritto in** | 100% Rust puro |
| **Pro** | Nessuna dipendenza C/C++, concurrent search/mutation |
| **Contro** | API piu' verbosa, meno community/download |
| **Persistence** | Si (`save_to` / `load_from`) |
| **Metriche** | L2, Cosine, InnerProduct (f32, f16, bf16, i8) |

Buona alternativa se volessimo zero C++, ma USearch e' piu' maturo e performante.

#### Opzione C: Brute-force in Rust puro — SCARTATA

| Aspetto | Dettaglio |
|---------|-----------|
| **Performance** | O(N) — ~1ms per 10k, ma **~50ms per 100k**, **~500ms per 1M** |
| **Scaling** | Lineare — degrada inevitabilmente |
| **Persistence** | Serve re-implementare save/load manualmente |
| **Indexing** | Nessuno — cerca tutto ogni volta |

**Perche' scartata**: "funziona ora" ma non scala. Un assistente personale attivo accumula ~10-50 chunk/giorno. In 2-3 anni = 10k-50k chunks. In 5+ anni o con piu' utenti, brute-force diventa il collo di bottiglia. I database vettoriali dedicati esistono per un motivo: HNSW da' O(log N) con recall > 95%.

#### Opzione D: LanceDB — SCARTATA

| Aspetto | Dettaglio |
|---------|-----------|
| **Pro** | Vector DB completo, IVF-PQ, full-text search, SQL |
| **Contro** | Richiede Apache Arrow + protobuf + lzma-sys |
| **Binary bloat** | Significativo (Arrow e' pesante) |
| **Complessita'** | Overengineered per il nostro caso |

**Perche' scartata**: troppe dipendenze transitive, binary bloat, richiede protobuf system dependency. Overkill per un assistente personale.

#### Opzione E: sqlite-vec — SCARTATA

**Perche' scartata**: richiede `rusqlite`, noi usiamo `sqlx`. Non esiste integrazione documentata. Due pool SQLite = architettura sporca.

#### Opzione F: Qdrant Edge — NON DISPONIBILE

In private beta (2025). Nessun crate pubblico ancora. Da monitorare per il futuro.

---

### Architettura Dati: Separazione di Responsabilita'

```
SQLite (sqlx) — Source of truth per il TESTO
├── memory_chunks (id, date, source, heading, content, created_at)
├── memory_fts (FTS5 virtual table per keyword search BM25)
└── trigger sync automatico FTS5

USearch file — Indice vettoriale per SIMILARITY SEARCH
└── ~/.homun/memory.usearch (file singolo, HNSW index)
    ├── key = chunk_id (i64 da SQLite)
    └── vector = embedding f32x384
```

**Perche' separare**:
- SQLite fa quello che sa fare meglio: testo, query, FTS5, ACID
- USearch fa quello che sa fare meglio: vector ANN search, O(log N), SIMD
- Nessuno dei due fa il lavoro dell'altro bene
- Fallback: se il file .usearch si corrompe, si ricostruisce da SQLite (gli embedding originali possono essere ri-generati dal testo)

### Keyword Search: SQLite FTS5 (gia' in sqlx)

FTS5 e' gia' disponibile nel nostro SQLite via sqlx. BM25 scoring integrato.

### Scoring Ibrido

```
score = 0.7 * cosine_similarity + 0.3 * normalized_bm25
```

Stesso approccio di ZeroClaw, ma tutto locale.

---

## Dipendenze da Aggiungere

```toml
fastembed = "5"       # Embedding locale ONNX (multilingue, ~110MB modello al primo uso)
usearch = "2"         # HNSW vector index (320KB, zero server)
```

**Due dipendenze**. fastembed per generare i vettori. USearch per cercarli efficientemente.

---

## Schema SQLite — Migration 002

```sql
-- Migration 002: Memory vector search
-- Chunks di memoria con testo (i vettori stanno in USearch)

CREATE TABLE IF NOT EXISTS memory_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,                    -- "2026-02-21"
    source TEXT NOT NULL DEFAULT 'daily',  -- "daily", "consolidation", "user"
    heading TEXT,                          -- heading context dal markdown
    content TEXT NOT NULL,                 -- testo del chunk
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_chunks_date ON memory_chunks(date);
CREATE INDEX IF NOT EXISTS idx_chunks_source ON memory_chunks(source);

-- FTS5 per keyword search con BM25
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    content,
    heading,
    content='memory_chunks',
    content_rowid='id'
);

-- Trigger per mantenere FTS5 sincronizzato
CREATE TRIGGER IF NOT EXISTS memory_chunks_ai AFTER INSERT ON memory_chunks BEGIN
    INSERT INTO memory_fts(rowid, content, heading) VALUES (new.id, new.content, new.heading);
END;

CREATE TRIGGER IF NOT EXISTS memory_chunks_ad AFTER DELETE ON memory_chunks BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, content, heading)
    VALUES ('delete', old.id, old.content, old.heading);
END;

CREATE TRIGGER IF NOT EXISTS memory_chunks_au AFTER UPDATE ON memory_chunks BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, content, heading)
    VALUES ('delete', old.id, old.content, old.heading);
    INSERT INTO memory_fts(rowid, content, heading) VALUES (new.id, new.content, new.heading);
END;
```

**Nota**: embedding NON in SQLite. Stanno nel file USearch. SQLite tiene il testo + FTS5.

---

## Flusso Completo

```
WRITE PATH (dopo conversazione):
1. Utente parla con l'agente (N messaggi)
2. maybe_consolidate() detecta soglia superata
3. LLM produce: history_entry + memory_update (GIA' CODIFICATO in memory.rs)
4. history_entry → salvato in memory/YYYY-MM-DD.md (leggibile)
5. history_entry chunked → INSERT in memory_chunks (SQLite)
6. FTS5 aggiornato automaticamente (trigger)
7. Chunks embedded con fastembed → aggiunti a USearch index
8. USearch index.save() → ~/.homun/memory.usearch
9. MEMORY.md aggiornato (fatti a lungo termine)

READ PATH (prima di ogni risposta):
1. Messaggio utente arriva
2. Embed messaggio con fastembed → query_embedding
3. USearch index.search(query_embedding, 20) → top 20 chunk_ids + distances (O(log N))
4. FTS5 keyword search con parole chiave → top 20 chunk_ids + bm25 scores
5. Merge: score = 0.7 * (1 - distance) + 0.3 * normalized_bm25
6. Top 5 chunk_ids → SELECT content FROM memory_chunks WHERE id IN (...)
7. Chunks iniettati nel context dell'LLM come "## Relevant Memories"

STARTUP:
1. USearch Index::load("~/.homun/memory.usearch") (se esiste)
2. Se non esiste o corrotto: ricostruisci da SQLite memory_chunks + fastembed
```

---

## Piano di Implementazione — 4 Fasi

### Fase 1: Far Funzionare la Consolidazione (SENZA vector) — 1-2 ore

**Problema**: `maybe_consolidate()` in `agent_loop.rs:488` detecta la soglia ma NON chiama `memory.consolidate()`. C'e' un commento TODO sulla riga 505.

**Fix**:
1. L'`AgentLoop` ha gia' `Arc<MemoryConsolidator>` (riga 36)
2. Il provider e' `Arc<dyn Provider>` — passarlo a `consolidate()` con `.as_ref()`
3. Aggiungere logica per salvare in `memory/YYYY-MM-DD.md` oltre che in HISTORY.md
4. Testare: mandare 25+ messaggi → verificare che MEMORY.md viene scritto

**File da modificare**:
- `src/agent/agent_loop.rs` — `maybe_consolidate()`: aggiungere la chiamata reale
- `src/agent/memory.rs` — aggiungere metodo `save_daily_md()` per memoria giornaliera

### Fase 2: Aggiungere fastembed + USearch + Embedding Store — 2-3 ore

**Creare `src/agent/embeddings.rs`** (~200 righe):

```rust
use anyhow::Result;
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};
use std::path::PathBuf;
use tokio::sync::OnceCell;

/// Motore embedding + indice vettoriale.
/// Gestisce sia la generazione embedding (fastembed) che la ricerca (USearch).
pub struct EmbeddingEngine {
    model: OnceCell<TextEmbedding>,
    index: parking_lot::RwLock<Index>,
    index_path: PathBuf,
}

impl EmbeddingEngine {
    pub fn new(data_dir: &Path) -> Result<Self> {
        let index_path = data_dir.join("memory.usearch");

        // Crea o carica l'indice USearch
        let mut opts = IndexOptions::default();
        opts.dimensions = 384;
        opts.metric = MetricKind::Cos;
        opts.quantization = ScalarKind::F32;

        let index = Index::new(&opts)?;
        if index_path.exists() {
            index.load(index_path.to_str().unwrap())?;
        }

        Ok(Self {
            model: OnceCell::new(),
            index: parking_lot::RwLock::new(index),
            index_path,
        })
    }

    /// Embed + inserisci nel vector index
    pub async fn index_chunk(&self, chunk_id: i64, text: &str) -> Result<()> {
        let embedding = self.embed_one(text).await?;
        let index = self.index.write();
        index.add(chunk_id as u64, &embedding)?;
        Ok(())
    }

    /// Cerca i chunk piu' simili
    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<(i64, f32)>> {
        let query_embedding = self.embed_one(query).await?;
        let index = self.index.read();
        let results = index.search(&query_embedding, top_k)?;
        Ok(results.keys.iter().zip(results.distances.iter())
            .map(|(k, d)| (*k as i64, *d))
            .collect())
    }

    /// Salva l'indice su disco
    pub fn save(&self) -> Result<()> {
        let index = self.index.read();
        index.save(self.index_path.to_str().unwrap())?;
        Ok(())
    }
}
```

**Migration 002**: creare `migrations/002_memory_vectors.sql` con lo schema sopra.

**Aggiornare `db.rs`**: aggiungere metodi per insert/query memory_chunks (senza embedding — solo testo).

### Fase 3: Hybrid Search + Context Injection — 2-3 ore

**Creare `src/agent/memory_search.rs`** (~120 righe):

```rust
/// Ricerca ibrida: USearch vector + FTS5 keyword → scoring combinato
pub struct MemorySearcher {
    db: Database,
    engine: Arc<EmbeddingEngine>,
}

impl MemorySearcher {
    /// Cerca memorie rilevanti per un messaggio utente
    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<MemoryChunk>> {
        // 1. USearch: top 20 per similarity (O(log N))
        let vector_results = self.engine.search(query, 20).await?;

        // 2. FTS5: top 20 per keyword match (BM25)
        let keywords = extract_keywords(query);
        let fts_results = self.db.fts5_search(&keywords, 20).await?;

        // 3. Hybrid merge: 0.7 * (1 - distance) + 0.3 * bm25_normalized
        let merged_ids = hybrid_merge(&vector_results, &fts_results, top_k);

        // 4. Carica il contenuto testuale da SQLite
        self.db.load_chunks_by_ids(&merged_ids).await
    }
}
```

**Integrare in `context.rs`**: aggiungere Layer 3.5 — "Relevant Memories" tra long-term memory e guidelines.

### Fase 4: Indicizzazione nella Consolidazione — 1-2 ore

**Aggiornare `memory.rs`**: dopo il consolidamento, indicizza i chunk:

```rust
// In consolidate(), dopo aver salvato history_entry:
if !history_entry.is_empty() {
    let chunks = chunk_text(&history_entry, &date_str);
    for chunk in &chunks {
        let chunk_id = self.db.insert_chunk(chunk).await?;
        self.engine.index_chunk(chunk_id, &chunk.content).await?;
    }
    self.engine.save()?;
}
```

---

## Confronto con Competitor (aggiornato)

| Aspetto | OpenClaw | ZeroClaw | **Homun (piano)** |
|---------|----------|----------|-------------------|
| Storage testo | Markdown files | SQLite | **Markdown + SQLite** |
| Storage vettori | N/A | SQLite BLOB | **USearch (HNSW file)** |
| Algoritmo ricerca | Nessuno | Brute-force cosine | **HNSW O(log N)** |
| Embedding | Nessuno | API (OpenAI/custom) | **Locale (fastembed ONNX)** |
| Modello | N/A | Dipende dall'API | **ParaphraseMLMiniLML12V2Q** |
| Lingue | N/A | Dipende dall'API | **50+ lingue** |
| Keyword search | Nessuno | FTS5 BM25 | **FTS5 BM25** |
| Hybrid scoring | No | Si (0.7/0.3) | **Si (0.7/0.3)** |
| Leggibilita' | Si (markdown) | No (solo DB) | **Si (markdown + DB)** |
| Dipendenze API | N/A | Si (serve API) | **Zero (tutto locale)** |
| Offline | Si | No | **Si** |
| Hardware min. | Mac mini | Mac mini | **Qualsiasi CPU** |
| Scaling | Non scala | O(N) | **O(log N)** |

### Il nostro vantaggio unico

1. **Embedding completamente locale** — ZeroClaw richiede un'API esterna
2. **HNSW O(log N)** — ZeroClaw fa brute-force O(N)
3. **Multilingue** — 50+ lingue out-of-the-box
4. **Markdown leggibili** — OpenClaw style, l'utente puo' leggere/editare la memoria
5. **Due dipendenze** — fastembed + usearch, nient'altro
6. **Funziona offline** — aereo, treno, montagna
7. **Qualsiasi hardware** — dal Raspberry Pi 4 al Mac Studio

---

## Rischi e Mitigazioni

| Rischio | Impatto | Mitigazione |
|---------|---------|-------------|
| fastembed aggiunge ~10MB al binary | Basso | Accettabile, gia' pesante con ONNX |
| usearch richiede cxx (C++ binding) | Basso | cxx e' maturo, compila ovunque |
| Download modello ~110MB al primo uso | Medio | Progress bar, messaggio chiaro |
| ONNX init lento (2-3 sec) | Basso | Lazy init, una volta sola |
| File .usearch separato da SQLite | Basso | Ricostruibile da testo + fastembed |
| FTS5 non in sqlx default | Medio | Verificare feature flag; FTS5 standard in SQLite 3.9+ |

---

## File Coinvolti (sommario)

| File | Azione | Fase |
|------|--------|------|
| `Cargo.toml` | +fastembed, +usearch | 2 |
| `migrations/002_memory_vectors.sql` | Nuovo | 2 |
| `src/agent/agent_loop.rs` | Fix maybe_consolidate() | 1 |
| `src/agent/memory.rs` | +save_daily_md(), +chunking, +embed call | 1,4 |
| `src/agent/embeddings.rs` | Nuovo (~200 righe) | 2 |
| `src/agent/memory_search.rs` | Nuovo (~120 righe) | 3 |
| `src/agent/context.rs` | +Layer "Relevant Memories" | 3 |
| `src/agent/mod.rs` | pub use nuovi moduli | 2,3 |
| `src/storage/db.rs` | +insert_chunk, +load_chunks, +fts5_search | 2,3 |

**Totale nuovo codice stimato**: ~500-600 righe Rust + ~30 righe SQL
