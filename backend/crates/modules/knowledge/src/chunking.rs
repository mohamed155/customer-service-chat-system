use std::collections::HashSet;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct ChunkResult {
    pub chunks: Vec<Chunk>,
    pub content_hash: String,
    pub not_indexable: bool,
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub ordinal: usize,
    pub content: String,
}

const CHUNK_MIN_CHARS: usize = 2000;
const CHUNK_MAX_CHARS: usize = 3200;
const OVERLAP_CHARS: usize = 400;
const MAX_CHUNKS: usize = 500;

/// Extracts plain text from a knowledge item's body based on its content type.
///
/// Returns `None` when the content type is unsupported, the body is not valid
/// UTF-8 (for non-PDF types), or the extracted text is empty/whitespace-only.
pub fn extract_text(content_type: &str, body: &[u8]) -> Option<String> {
    let text = if content_type.contains("html") {
        let html = std::str::from_utf8(body).ok()?;
        html_to_text(html)
    } else if content_type.starts_with("text/") {
        std::str::from_utf8(body).ok()?.to_string()
    } else if content_type == "application/pdf" {
        pdf_extract::extract_text_from_mem(body).ok()?
    } else {
        return None;
    };

    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Chunks text into deterministic, paragraph/sentence-aware segments.
///
/// Returns a `ChunkResult` with the content hash, the list of chunks, and a
/// `not_indexable` flag when the input is empty or whitespace-only.
pub fn chunk_text(text: &str) -> ChunkResult {
    let content_hash = hex::encode(Sha256::digest(text.as_bytes()));

    if text.trim().is_empty() {
        return ChunkResult {
            chunks: vec![],
            content_hash,
            not_indexable: true,
        };
    }

    let base_chunks = build_base_chunks(text);
    let count = base_chunks.len().min(MAX_CHUNKS);
    let mut chunks = Vec::with_capacity(count);
    let mut prev_tail = String::new();

    for (ordinal, base) in base_chunks.iter().enumerate().take(count) {
        let content = if ordinal == 0 || prev_tail.is_empty() {
            base.clone()
        } else {
            format!("{}\n\n{}", prev_tail, base)
        };

        prev_tail = tail_overlap(base);

        chunks.push(Chunk { ordinal, content });
    }

    ChunkResult {
        chunks,
        content_hash,
        not_indexable: false,
    }
}

// --- HTML → plain text ---

fn html_to_text(html: &str) -> String {
    let text = ammonia::Builder::new()
        .tags(HashSet::new())
        .clean(html)
        .to_string();

    normalize_whitespace(&text)
}

fn normalize_whitespace(text: &str) -> String {
    let paragraphs: Vec<&str> = text
        .split('\n')
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    if paragraphs.is_empty() {
        return String::new();
    }

    paragraphs.join("\n\n")
}

// --- Chunk building ---

fn build_base_chunks(text: &str) -> Vec<String> {
    let paragraphs = split_paragraphs(text);
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for para in &paragraphs {
        let candidate = if current.is_empty() {
            para.clone()
        } else {
            format!("{}\n\n{}", current, para)
        };

        let fits = candidate.len() <= CHUNK_MAX_CHARS
            || (current.len() < CHUNK_MIN_CHARS
                && candidate.len() <= CHUNK_MAX_CHARS * 2);

        if fits {
            current = candidate;
        } else if !current.is_empty() {
            chunks.push(current);

            if para.len() > CHUNK_MAX_CHARS {
                for sub in split_long_paragraph(para) {
                    chunks.push(sub);
                }
                current = String::new();
            } else {
                current = para.clone();
            }
        } else {
            current = para.clone();
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn split_paragraphs(text: &str) -> Vec<String> {
    text.split("\n\n")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn split_long_paragraph(text: &str) -> Vec<String> {
    let sentences = split_sentences(text);
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for sentence in &sentences {
        let candidate = if current.is_empty() {
            sentence.clone()
        } else {
            format!("{} {}", current, sentence)
        };

        if candidate.len() <= CHUNK_MAX_CHARS {
            current = candidate;
        } else {
            if !current.is_empty() {
                chunks.push(current);
            }
            current = sentence.clone();
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();

    for i in 0..len {
        let ch = bytes[i];
        if (ch == b'.' || ch == b'!' || ch == b'?') && i + 1 < len && bytes[i + 1] == b' ' {
            result.push(text[start..=i + 1].to_string());
            start = i + 2;
        } else if (ch == b'.' || ch == b'!' || ch == b'?') && i == len - 1 {
            result.push(text[start..=i].to_string());
            start = i + 1;
        }
    }

    if start < len {
        let remainder = text[start..].trim();
        if !remainder.is_empty() {
            result.push(remainder.to_string());
        }
    }

    result
}

// --- Overlap extraction ---

fn tail_overlap(text: &str) -> String {
    if text.len() <= OVERLAP_CHARS {
        return String::new();
    }

    let tail_start = text.len() - OVERLAP_CHARS;
    let tail = &text[tail_start..];

    if let Some(pos) = tail.find(['.', '!', '?']) {
        let abs = tail_start + pos + 1;
        if abs > tail_start && abs < text.len() {
            let after = text[abs..].trim_start();
            if after.is_empty() {
                return String::new();
            }
            return after.to_string();
        }
    }

    tail.trim_start().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_text ---

    #[test]
    fn extract_text_unknown_type_returns_none() {
        assert_eq!(extract_text("application/octet-stream", b"data"), None);
    }

    #[test]
    fn extract_text_plain_text_returns_as_is() {
        let result = extract_text("text/plain", b"hello world");
        assert_eq!(result.as_deref(), Some("hello world"));
    }

    #[test]
    fn extract_text_plain_text_empty_returns_none() {
        assert_eq!(extract_text("text/plain", b""), None);
    }

    #[test]
    fn extract_text_plain_text_whitespace_returns_none() {
        assert_eq!(extract_text("text/plain", b"   \n  "), None);
    }

    #[test]
    fn extract_text_markdown_returns_as_is() {
        let result = extract_text("text/markdown", b"# Hello\n\nworld");
        assert_eq!(result.as_deref(), Some("# Hello\n\nworld"));
    }

    #[test]
    fn extract_text_html_strips_tags() {
        let result = extract_text("text/html", b"<p>Hello</p><p>World</p>");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn extract_text_pdf_nonexistent_returns_none() {
        let result = extract_text("application/pdf", b"not a pdf");
        assert_eq!(result, None);
    }

    // --- chunk_text ---

    #[test]
    fn chunk_text_empty_returns_not_indexable() {
        let result = chunk_text("");
        assert!(result.not_indexable);
        assert!(result.chunks.is_empty());
        assert!(!result.content_hash.is_empty());
    }

    #[test]
    fn chunk_text_whitespace_returns_not_indexable() {
        let result = chunk_text("   \n  ");
        assert!(result.not_indexable);
        assert!(result.chunks.is_empty());
    }

    #[test]
    fn chunk_text_short_text_returns_one_chunk() {
        let result = chunk_text("Hello, world!");
        assert!(!result.not_indexable);
        assert_eq!(result.chunks.len(), 1);
        assert_eq!(result.chunks[0].ordinal, 0);
        assert_eq!(result.chunks[0].content, "Hello, world!");
    }

    #[test]
    fn chunk_text_deterministic() {
        let text = "This is a test.\n\nIt has multiple paragraphs.\n\nFor testing determinism.";
        let a = chunk_text(text);
        let b = chunk_text(text);
        assert_eq!(a.chunks.len(), b.chunks.len());
        assert_eq!(a.content_hash, b.content_hash);
        for (ca, cb) in a.chunks.iter().zip(b.chunks.iter()) {
            assert_eq!(ca.content, cb.content);
        }
    }

    #[test]
    fn chunk_text_content_hash_is_sha256() {
        let text = "content hash test";
        let result = chunk_text(text);
        let expected = hex::encode(Sha256::digest(text.as_bytes()));
        assert_eq!(result.content_hash, expected);
    }

    #[test]
    fn chunk_text_paragraph_boundaries() {
        let text = "Paragraph one.\n\nParagraph two.\n\nParagraph three.";
        let result = chunk_text(text);
        assert!(!result.chunks.is_empty());
        assert!(result.chunks[0].content.contains("Paragraph one."));
    }

    #[test]
    fn chunk_text_max_chunks() {
        let para = "This is a short paragraph. ".repeat(50);
        let many: String = std::iter::repeat(para.as_str())
            .take(MAX_CHUNKS + 50)
            .collect::<Vec<_>>()
            .join("\n\n");
        let result = chunk_text(&many);
        assert!(result.chunks.len() <= MAX_CHUNKS);
    }

    // --- split_sentences ---

    #[test]
    fn split_sentences_basic() {
        let s = "First sentence. Second sentence! Third? Fourth.";
        let result = split_sentences(s);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn split_sentences_no_punctuation() {
        let s = "Just one blob of text without punctuation";
        let result = split_sentences(s);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn split_sentences_trailing_text() {
        let s = "First. Second.Third without space.";
        let result = split_sentences(s);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "First. ");
        assert_eq!(result[1], "Second.Third without space.");
    }
}
