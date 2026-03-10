//! Binary document parsers for RAG ingestion (PDF, DOCX, XLSX).

use std::path::Path;

use anyhow::{Context as _, Result};

use super::chunker::{ChunkOptions, DocChunk};

/// Dispatch binary file parsing by document type.
pub fn chunk_binary(path: &Path, doc_type: &str, opts: &ChunkOptions) -> Result<Vec<DocChunk>> {
    match doc_type {
        "pdf" => chunk_pdf(path, opts),
        "docx" => chunk_docx(path, opts),
        "spreadsheet" => chunk_spreadsheet(path, opts),
        _ => anyhow::bail!("Unsupported binary doc type: {doc_type}"),
    }
}

// ─── PDF ────────────────────────────────────────────────────────

fn chunk_pdf(path: &Path, opts: &ChunkOptions) -> Result<Vec<DocChunk>> {
    let mut text = pdf_extract::extract_text(path)
        .with_context(|| format!("Failed to extract text from PDF {}", path.display()))?;

    // If no text extracted, try OCR via tesseract CLI (handles image-based PDFs)
    if text.trim().is_empty() {
        match try_ocr_pdf(path) {
            Ok(ocr_text) if !ocr_text.trim().is_empty() => {
                tracing::info!(path = %path.display(), "PDF text empty, used OCR fallback");
                text = ocr_text;
            }
            Ok(_) => {
                tracing::warn!(
                    path = %path.display(),
                    "PDF contains no extractable text and OCR produced no results (image-based PDF?)"
                );
                return Ok(Vec::new());
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "PDF contains no extractable text and OCR is unavailable. \
                     Install tesseract for image-based PDF support."
                );
                return Ok(Vec::new());
            }
        }
    }

    // Split on form feeds (\x0C) for page-aware chunking
    let pages: Vec<&str> = text.split('\x0C').collect();
    let mut chunks = Vec::new();

    for (page_num, page_text) in pages.iter().enumerate() {
        let trimmed = page_text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let heading = format!("Page {}", page_num + 1);
        super::chunker::split_into_chunks(trimmed, &heading, opts, &mut chunks);
    }

    for (i, chunk) in chunks.iter_mut().enumerate() {
        chunk.index = i;
    }
    Ok(chunks)
}

/// Try OCR on a PDF using tesseract CLI.
/// Converts PDF pages to images via pdftoppm, then runs tesseract on each.
fn try_ocr_pdf(path: &Path) -> Result<String> {
    // Check if tesseract is available
    let check = std::process::Command::new("tesseract")
        .arg("--version")
        .output();
    if check.is_err() || !check.unwrap().status.success() {
        anyhow::bail!("tesseract not found");
    }

    // Check if pdftoppm is available (from poppler-utils)
    let check_ppm = std::process::Command::new("pdftoppm")
        .arg("-h")
        .output();
    if check_ppm.is_err() {
        anyhow::bail!("pdftoppm not found (install poppler)");
    }

    let tmp_dir = tempfile::tempdir().context("Failed to create temp dir for OCR")?;

    // Convert PDF to PNG images
    let ppm_status = std::process::Command::new("pdftoppm")
        .args(["-png", "-r", "300"])
        .arg(path)
        .arg(tmp_dir.path().join("page").to_str().unwrap_or("page"))
        .status()
        .context("Failed to run pdftoppm")?;

    if !ppm_status.success() {
        anyhow::bail!("pdftoppm failed with status {}", ppm_status);
    }

    // Find generated page images and OCR each
    let mut pages: Vec<std::path::PathBuf> = std::fs::read_dir(tmp_dir.path())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("png"))
        .collect();
    pages.sort();

    let mut all_text = String::new();
    for page_img in &pages {
        let output = std::process::Command::new("tesseract")
            .arg(page_img)
            .arg("stdout")
            .arg("-l")
            .arg("eng+ita")
            .output()
            .context("Failed to run tesseract")?;

        if output.status.success() {
            let page_text = String::from_utf8_lossy(&output.stdout);
            if !page_text.trim().is_empty() {
                all_text.push_str(&page_text);
                all_text.push('\x0C'); // Form feed for page separation
            }
        }
    }

    Ok(all_text)
}

// ─── DOCX ───────────────────────────────────────────────────────

fn chunk_docx(path: &Path, opts: &ChunkOptions) -> Result<Vec<DocChunk>> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open DOCX {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("Invalid DOCX archive {}", path.display()))?;

    let mut xml_content = String::new();
    if let Ok(mut entry) = archive.by_name("word/document.xml") {
        use std::io::Read;
        entry
            .read_to_string(&mut xml_content)
            .with_context(|| "Failed to read word/document.xml")?;
    } else {
        anyhow::bail!("No word/document.xml found in DOCX");
    }

    let text = extract_text_from_docx_xml(&xml_content);
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    super::chunker::chunk_text(&text, "text", opts)
}

/// Extract plain text from DOCX XML by reading <w:t> elements within <w:p> paragraphs.
fn extract_text_from_docx_xml(xml: &str) -> String {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    let mut text = String::new();
    let mut in_text_el = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e))
                if e.local_name().as_ref() == b"t" =>
            {
                in_text_el = true;
            }
            Ok(Event::End(e)) if e.local_name().as_ref() == b"t" => {
                in_text_el = false;
            }
            Ok(Event::Text(e)) if in_text_el => {
                if let Ok(s) = e.unescape() {
                    text.push_str(&s);
                }
            }
            Ok(Event::End(e)) if e.local_name().as_ref() == b"p" => {
                text.push('\n');
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    text
}

// ─── Spreadsheets (XLSX, XLS) ───────────────────────────────────

fn chunk_spreadsheet(path: &Path, opts: &ChunkOptions) -> Result<Vec<DocChunk>> {
    use calamine::{open_workbook_auto, Reader};

    let mut workbook = open_workbook_auto(path)
        .with_context(|| format!("Failed to open spreadsheet {}", path.display()))?;

    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();
    let mut chunks = Vec::new();

    for sheet_name in &sheet_names {
        if let Ok(range) = workbook.worksheet_range(sheet_name) {
            let mut rows_text = Vec::new();
            for row in range.rows() {
                let cells: Vec<String> = row.iter().map(|c| format_cell(c)).collect();
                let line = cells.join(" | ");
                if !line.trim().is_empty() && line.trim() != "|" {
                    rows_text.push(line);
                }
            }
            if rows_text.is_empty() {
                continue;
            }
            let sheet_text = rows_text.join("\n");
            let heading = format!("Sheet: {}", sheet_name);
            super::chunker::split_into_chunks(&sheet_text, &heading, opts, &mut chunks);
        }
    }

    for (i, chunk) in chunks.iter_mut().enumerate() {
        chunk.index = i;
    }
    Ok(chunks)
}

/// Format a calamine cell value, trimming empty cells.
fn format_cell(cell: &calamine::Data) -> String {
    match cell {
        calamine::Data::Empty => String::new(),
        calamine::Data::String(s) => s.clone(),
        calamine::Data::Float(f) => {
            if *f == (*f as i64) as f64 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        calamine::Data::Int(i) => format!("{i}"),
        calamine::Data::Bool(b) => format!("{b}"),
        calamine::Data::DateTime(dt) => format!("{dt}"),
        calamine::Data::DateTimeIso(s) => s.clone(),
        calamine::Data::DurationIso(s) => s.clone(),
        calamine::Data::Error(e) => format!("#ERR:{e:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_docx_xml() {
        let xml = r#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
            <w:body>
                <w:p><w:r><w:t>Hello world</w:t></w:r></w:p>
                <w:p><w:r><w:t>Second paragraph</w:t></w:r></w:p>
            </w:body>
        </w:document>"#;

        let text = extract_text_from_docx_xml(xml);
        assert!(text.contains("Hello world"));
        assert!(text.contains("Second paragraph"));
    }

    #[test]
    fn test_format_cell() {
        assert_eq!(format_cell(&calamine::Data::String("test".into())), "test");
        assert_eq!(format_cell(&calamine::Data::Float(42.0)), "42");
        assert_eq!(format_cell(&calamine::Data::Float(3.14)), "3.14");
        assert_eq!(format_cell(&calamine::Data::Int(100)), "100");
        assert_eq!(format_cell(&calamine::Data::Bool(true)), "true");
        assert_eq!(format_cell(&calamine::Data::Empty), "");
    }
}
