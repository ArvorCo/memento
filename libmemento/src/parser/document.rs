/// Parser for various document formats: PDF, TXT, MD, CSV, code files
use anyhow::{Context, Result};
use std::path::Path;

pub struct DocumentParser;

impl DocumentParser {
    pub fn new() -> Self {
        DocumentParser
    }

    /// Parse a file and return its text content
    pub fn parse_file(&self, path: &Path) -> Result<String> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "pdf" => self.parse_pdf(path),
            "doc" | "docx" | "odt" | "ppt" | "pptx" | "rtf" | "xls" | "xlsx" => {
                anyhow::bail!(
                    "{} is a binary office document; convert it with `memento-vault-sync import-documents` before direct import",
                    path.display()
                )
            }
            _ => self.parse_text(path),
        }
    }

    fn parse_pdf(&self, path: &Path) -> Result<String> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read PDF: {}", path.display()))?;

        let text = pdf_extract::extract_text_from_mem(&bytes)
            .with_context(|| format!("Failed to extract text from PDF: {}", path.display()))?;
        if text
            .chars()
            .filter(|character| character.is_alphanumeric())
            .count()
            < 24
        {
            anyhow::bail!(
                "PDF has no useful text layer: {}; use the document feeder for local OCR fallback",
                path.display()
            );
        }
        Ok(normalize_extracted_text(&text))
    }

    fn parse_text(&self, path: &Path) -> Result<String> {
        std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))
    }
}

fn normalize_extracted_text(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = Vec::new();
    let mut previous_blank = false;
    for line in normalized.lines() {
        let line = line.trim_end();
        let blank = line.trim().is_empty();
        if blank && previous_blank {
            continue;
        }
        output.push(line);
        previous_blank = blank;
    }
    output.join("\n").trim().to_string()
}

impl Default for DocumentParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_text_file() {
        let mut file = NamedTempFile::with_suffix(".txt").unwrap();
        writeln!(file, "Hello, world!").unwrap();
        writeln!(file, "This is a test document.").unwrap();

        let parser = DocumentParser::new();
        let text = parser.parse_file(file.path()).unwrap();

        assert!(text.contains("Hello, world!"));
        assert!(text.contains("This is a test document."));
    }

    #[test]
    fn test_parse_markdown_file() {
        let mut file = NamedTempFile::with_suffix(".md").unwrap();
        writeln!(file, "# Title").unwrap();
        writeln!(file, "Some content here.").unwrap();

        let parser = DocumentParser::new();
        let text = parser.parse_file(file.path()).unwrap();

        assert!(text.contains("# Title"));
        assert!(text.contains("Some content here."));
    }

    #[test]
    fn test_parse_nonexistent_file() {
        let parser = DocumentParser::new();
        let result = parser.parse_file(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rust_file() {
        let mut file = NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(file, "fn main() {{").unwrap();
        writeln!(file, "    println!(\"Hello\");").unwrap();
        writeln!(file, "}}").unwrap();

        let parser = DocumentParser::new();
        let text = parser.parse_file(file.path()).unwrap();

        assert!(text.contains("fn main()"));
        assert!(text.contains("println!"));
    }

    #[test]
    fn test_binary_office_document_has_actionable_error() {
        let file = NamedTempFile::with_suffix(".docx").unwrap();

        let error = DocumentParser::new().parse_file(file.path()).unwrap_err();

        assert!(error
            .to_string()
            .contains("memento-vault-sync import-documents"));
    }

    #[test]
    fn normalize_extracted_text_collapses_pdf_noise() {
        assert_eq!(
            normalize_extracted_text("first  \r\n\r\n\r\nsecond\r\n"),
            "first\n\nsecond"
        );
    }
}
