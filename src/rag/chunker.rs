use std::path::Path;

use anyhow::{Context as _, Result};

/// A chunk extracted from a document.
#[derive(Debug, Clone)]
pub struct DocChunk {
    pub index: usize,
    pub heading: String,
    pub content: String,
    pub token_count: usize,
}

/// Options controlling chunking behavior.
#[derive(Debug, Clone)]
pub struct ChunkOptions {
    pub max_tokens: usize,
    pub overlap_tokens: usize,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            overlap_tokens: 50,
        }
    }
}

/// Supported file extensions for RAG ingestion.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "md", "markdown", "txt", "log", "rs", "py", "js", "ts", "go", "java", "c", "cpp", "h",
    "hpp", "toml", "yaml", "yml", "json", "html", "htm", "css", "sh", "bash", "zsh", "sql",
    "xml", "csv", "ini", "cfg", "conf", "env", "dockerfile", "makefile",
    // Binary document formats (require feature-gated parsers)
    "pdf", "docx", "xlsx", "xls", "xlsm", "odt",
];

/// Detect document type from file extension.
pub fn detect_doc_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("md" | "markdown") => "markdown",
        Some("txt" | "log") => "text",
        Some("rs" | "py" | "js" | "ts" | "go" | "java" | "c" | "cpp" | "h" | "hpp" | "css"
            | "sh" | "bash" | "zsh" | "sql") => "code",
        Some("toml" | "yaml" | "yml" | "json" | "xml" | "ini" | "cfg" | "conf" | "env"
            | "csv") => "config",
        Some("html" | "htm") => "html",
        Some("pdf") => "pdf",
        Some("docx" | "odt") => "docx",
        Some("xlsx" | "xls" | "xlsm") => "spreadsheet",
        _ => "text",
    }
}

/// Check if a file extension is supported for RAG ingestion.
pub fn is_supported(path: &Path) -> bool {
    // Also check for extensionless files with known names
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    if matches!(file_name.as_str(), "dockerfile" | "makefile") {
        return true;
    }

    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|&s| s.eq_ignore_ascii_case(ext))
        })
        .unwrap_or(false)
}

/// Read and chunk a file. Returns empty vec for empty files.
/// Binary formats (PDF, DOCX, XLSX) are dispatched to specialized parsers.
pub fn chunk_file(path: &Path, opts: &ChunkOptions) -> Result<Vec<DocChunk>> {
    let doc_type = detect_doc_type(path);

    // Binary formats need specialized parsers
    match doc_type {
        "pdf" | "docx" | "spreadsheet" => {
            return super::parsers::chunk_binary(path, doc_type, opts);
        }
        _ => {}
    }

    // Text-based formats
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    if content.trim().is_empty() {
        return Ok(Vec::new());
    }

    chunk_text(&content, doc_type, opts)
}

/// Chunk text content by document type.
pub fn chunk_text(content: &str, doc_type: &str, opts: &ChunkOptions) -> Result<Vec<DocChunk>> {
    let chunks = match doc_type {
        "markdown" => chunk_markdown(content, opts),
        "code" => chunk_code(content, opts),
        "html" => chunk_html(content, opts),
        _ => chunk_plain_text(content, opts),
    };
    Ok(chunks)
}

/// Estimate tokens from text (~4 chars per token).
fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}

// ─── Markdown Chunking ──────────────────────────────────────────

fn chunk_markdown(content: &str, opts: &ChunkOptions) -> Vec<DocChunk> {
    let mut chunks = Vec::new();
    let mut current_heading = String::new();
    let mut current_lines: Vec<&str> = Vec::new();

    for line in content.lines() {
        if line.starts_with("## ") || line.starts_with("# ") {
            // Flush previous section
            if !current_lines.is_empty() {
                let text = current_lines.join("\n");
                split_into_chunks(&text, &current_heading, opts, &mut chunks);
                current_lines.clear();
            }
            current_heading = line.trim_start_matches('#').trim().to_string();
            current_lines.push(line);
        } else {
            current_lines.push(line);
        }
    }

    // Flush remaining
    if !current_lines.is_empty() {
        let text = current_lines.join("\n");
        split_into_chunks(&text, &current_heading, opts, &mut chunks);
    }

    // Re-index
    for (i, chunk) in chunks.iter_mut().enumerate() {
        chunk.index = i;
    }
    chunks
}

// ─── Code Chunking ──────────────────────────────────────────────

fn chunk_code(content: &str, opts: &ChunkOptions) -> Vec<DocChunk> {
    let mut chunks = Vec::new();
    let mut current_block: Vec<&str> = Vec::new();
    let mut blank_count = 0;

    for line in content.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            current_block.push(line);

            // Split on double blank line (top-level boundary)
            if blank_count >= 2 && !current_block.is_empty() {
                let text = current_block.join("\n").trim().to_string();
                if !text.is_empty() {
                    split_into_chunks(&text, "", opts, &mut chunks);
                }
                current_block.clear();
                blank_count = 0;
            }
        } else {
            blank_count = 0;
            current_block.push(line);
        }
    }

    // Flush remaining
    let text = current_block.join("\n").trim().to_string();
    if !text.is_empty() {
        split_into_chunks(&text, "", opts, &mut chunks);
    }

    for (i, chunk) in chunks.iter_mut().enumerate() {
        chunk.index = i;
    }
    chunks
}

// ─── HTML Chunking ──────────────────────────────────────────────

fn chunk_html(content: &str, opts: &ChunkOptions) -> Vec<DocChunk> {
    // Strip HTML tags, then chunk as plain text
    let stripped = strip_html_tags(content);
    chunk_plain_text(&stripped, opts)
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                // Add space after block-level tags
                result.push(' ');
            }
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    // Collapse whitespace
    let mut collapsed = String::with_capacity(result.len());
    let mut prev_space = false;
    for ch in result.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                collapsed.push(if ch == '\n' { '\n' } else { ' ' });
            }
            prev_space = true;
        } else {
            collapsed.push(ch);
            prev_space = false;
        }
    }

    collapsed
}

// ─── Plain Text Chunking ────────────────────────────────────────

fn chunk_plain_text(content: &str, opts: &ChunkOptions) -> Vec<DocChunk> {
    let mut chunks = Vec::new();
    split_into_chunks(content, "", opts, &mut chunks);
    for (i, chunk) in chunks.iter_mut().enumerate() {
        chunk.index = i;
    }
    chunks
}

// ─── Shared splitting logic ─────────────────────────────────────

/// Split text into chunks respecting paragraph boundaries and max_tokens.
/// Tries to split on `\n\n` first, then on `\n` if paragraphs are too large.
pub(crate) fn split_into_chunks(
    text: &str,
    heading: &str,
    opts: &ChunkOptions,
    out: &mut Vec<DocChunk>,
) {
    let tokens = estimate_tokens(text);
    if tokens <= opts.max_tokens {
        out.push(DocChunk {
            index: 0,
            heading: heading.to_string(),
            content: text.to_string(),
            token_count: tokens,
        });
        return;
    }

    // Split on paragraphs first
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    let mut current = String::new();

    for para in paragraphs {
        let para_tokens = estimate_tokens(para);

        // If a single paragraph exceeds max_tokens, split it by lines
        if para_tokens > opts.max_tokens {
            // Flush current buffer
            if !current.trim().is_empty() {
                let t = current.trim().to_string();
                out.push(DocChunk {
                    index: 0,
                    heading: heading.to_string(),
                    content: t.clone(),
                    token_count: estimate_tokens(&t),
                });
                current.clear();
            }
            // Split large paragraph by lines
            split_by_lines(para, heading, opts, out);
            continue;
        }

        let combined_tokens = estimate_tokens(&current) + para_tokens + 1;
        if combined_tokens > opts.max_tokens && !current.trim().is_empty() {
            // Flush current
            let t = current.trim().to_string();
            out.push(DocChunk {
                index: 0,
                heading: heading.to_string(),
                content: t.clone(),
                token_count: estimate_tokens(&t),
            });

            // Start new chunk with overlap from end of previous
            current = apply_overlap(&t, opts);
        }

        if !current.is_empty() && !current.ends_with('\n') {
            current.push_str("\n\n");
        }
        current.push_str(para);
    }

    // Flush remaining
    if !current.trim().is_empty() {
        let t = current.trim().to_string();
        out.push(DocChunk {
            index: 0,
            heading: heading.to_string(),
            content: t.clone(),
            token_count: estimate_tokens(&t),
        });
    }
}

/// Split a large block of text by individual lines when paragraphs are too big.
fn split_by_lines(text: &str, heading: &str, opts: &ChunkOptions, out: &mut Vec<DocChunk>) {
    let mut current = String::new();

    for line in text.lines() {
        let line_tokens = estimate_tokens(line);
        let combined = estimate_tokens(&current) + line_tokens + 1;

        if combined > opts.max_tokens && !current.trim().is_empty() {
            let t = current.trim().to_string();
            out.push(DocChunk {
                index: 0,
                heading: heading.to_string(),
                content: t.clone(),
                token_count: estimate_tokens(&t),
            });
            current = apply_overlap(&t, opts);
        }

        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }

    if !current.trim().is_empty() {
        let t = current.trim().to_string();
        out.push(DocChunk {
            index: 0,
            heading: heading.to_string(),
            content: t.clone(),
            token_count: estimate_tokens(&t),
        });
    }
}

/// Extract trailing text (~overlap_tokens worth) for chunk overlap.
fn apply_overlap(text: &str, opts: &ChunkOptions) -> String {
    if opts.overlap_tokens == 0 {
        return String::new();
    }

    let overlap_chars = opts.overlap_tokens * 4;
    if text.len() <= overlap_chars {
        return String::new();
    }

    // Find a good break point (newline or space)
    let start = text.len() - overlap_chars;
    let break_at = text[start..]
        .find('\n')
        .or_else(|| text[start..].find(' '))
        .map(|pos| start + pos + 1)
        .unwrap_or(start);

    text[break_at..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_doc_type() {
        assert_eq!(detect_doc_type(Path::new("file.md")), "markdown");
        assert_eq!(detect_doc_type(Path::new("file.rs")), "code");
        assert_eq!(detect_doc_type(Path::new("file.txt")), "text");
        assert_eq!(detect_doc_type(Path::new("file.toml")), "config");
        assert_eq!(detect_doc_type(Path::new("file.html")), "html");
        assert_eq!(detect_doc_type(Path::new("file.unknown")), "text");
    }

    #[test]
    fn test_is_supported() {
        assert!(is_supported(Path::new("file.md")));
        assert!(is_supported(Path::new("file.rs")));
        assert!(is_supported(Path::new("file.py")));
        assert!(is_supported(Path::new("Dockerfile")));
        assert!(is_supported(Path::new("Makefile")));
        assert!(is_supported(Path::new("file.pdf")));
        assert!(is_supported(Path::new("file.docx")));
        assert!(is_supported(Path::new("file.xlsx")));
        assert!(!is_supported(Path::new("file.zip")));
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
    }

    #[test]
    fn test_chunk_small_text() {
        let opts = ChunkOptions::default();
        let chunks = chunk_text("Hello world", "text", &opts).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, "Hello world");
        assert_eq!(chunks[0].index, 0);
    }

    #[test]
    fn test_chunk_markdown_headings() {
        let opts = ChunkOptions {
            max_tokens: 50,
            overlap_tokens: 0,
        };
        let md = "# Title\n\nIntro paragraph.\n\n## Section A\n\nContent A here.\n\n## Section B\n\nContent B here.";
        let chunks = chunk_text(md, "markdown", &opts).unwrap();
        assert!(chunks.len() >= 2);
        assert!(chunks[0].heading.is_empty() || chunks[0].heading == "Title");
    }

    #[test]
    fn test_chunk_plain_text_large() {
        let opts = ChunkOptions {
            max_tokens: 20,
            overlap_tokens: 0,
        };
        // ~80 chars = ~20 tokens each paragraph
        let text = "Paragraph one with some content here.\n\nParagraph two with different content.\n\nParagraph three final.";
        let chunks = chunk_text(text, "text", &opts).unwrap();
        assert!(chunks.len() >= 2);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.index, i);
        }
    }

    #[test]
    fn test_strip_html_tags() {
        let html = "<h1>Title</h1><p>Content <b>bold</b> text</p>";
        let stripped = strip_html_tags(html);
        assert!(stripped.contains("Title"));
        assert!(stripped.contains("Content"));
        assert!(stripped.contains("bold"));
        assert!(!stripped.contains('<'));
    }

    #[test]
    fn test_empty_file() {
        let tmp = PathBuf::from("/tmp/homun_test_empty.txt");
        std::fs::write(&tmp, "").unwrap();
        let chunks = chunk_file(&tmp, &ChunkOptions::default()).unwrap();
        assert!(chunks.is_empty());
        std::fs::remove_file(&tmp).ok();
    }
}
