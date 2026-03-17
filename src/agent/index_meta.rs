//! Sidecar metadata for HNSW vector indices.
//!
//! Each `.usearch` index file has a companion `.usearch.meta` JSON file
//! that records which embedding provider/model/dimensions were used to
//! build the index. This allows detecting mismatches when the user
//! changes embedding configuration in Settings.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

/// Metadata about an HNSW vector index — which embedding model built it.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IndexMeta {
    /// Embedding provider name (e.g. "ollama", "openai", "mistral").
    pub provider: String,
    /// Embedding model name (e.g. "nomic-embed-text", "text-embedding-3-small").
    pub model: String,
    /// Vector dimensions (e.g. 384, 768, 1536).
    pub dimensions: usize,
    /// Number of vectors in the index when last saved.
    pub chunk_count: usize,
    /// ISO 8601 timestamp of last save.
    pub built_at: String,
}

impl IndexMeta {
    /// Derive the `.meta` sidecar path from the index path.
    ///
    /// `memory.usearch` → `memory.usearch.meta`
    pub fn meta_path(index_path: &Path) -> PathBuf {
        let mut p = index_path.as_os_str().to_os_string();
        p.push(".meta");
        PathBuf::from(p)
    }

    /// Read metadata from the sidecar file.
    ///
    /// Returns `None` if the file doesn't exist or can't be parsed
    /// (e.g. pre-existing index from before this feature).
    pub fn read(index_path: &Path) -> Option<Self> {
        let meta_path = Self::meta_path(index_path);
        let data = std::fs::read_to_string(&meta_path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Write metadata to the sidecar file.
    pub fn write(&self, index_path: &Path) -> Result<()> {
        let meta_path = Self::meta_path(index_path);

        // Ensure parent dir exists
        if let Some(parent) = meta_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create dir for {}", meta_path.display()))?;
        }

        let json = serde_json::to_string_pretty(self).context("Failed to serialize IndexMeta")?;
        std::fs::write(&meta_path, json)
            .with_context(|| format!("Failed to write {}", meta_path.display()))?;

        Ok(())
    }

    /// Delete the sidecar file (used during index reset).
    pub fn delete(index_path: &Path) {
        let meta_path = Self::meta_path(index_path);
        let _ = std::fs::remove_file(&meta_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meta_path() {
        let path = Path::new("/home/user/.homun/memory.usearch");
        let meta = IndexMeta::meta_path(path);
        assert_eq!(meta, Path::new("/home/user/.homun/memory.usearch.meta"));
    }

    #[test]
    fn test_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("test.usearch");

        let meta = IndexMeta {
            provider: "ollama".into(),
            model: "nomic-embed-text".into(),
            dimensions: 384,
            chunk_count: 42,
            built_at: "2026-03-17T10:00:00Z".into(),
        };

        meta.write(&index_path).unwrap();

        let loaded = IndexMeta::read(&index_path).expect("should read back");
        assert_eq!(loaded.provider, "ollama");
        assert_eq!(loaded.model, "nomic-embed-text");
        assert_eq!(loaded.dimensions, 384);
        assert_eq!(loaded.chunk_count, 42);
    }

    #[test]
    fn test_read_missing() {
        let path = Path::new("/nonexistent/index.usearch");
        assert!(IndexMeta::read(path).is_none());
    }
}
