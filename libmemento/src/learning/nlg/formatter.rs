//! Markdown formatter for responses.

use crate::learning::error::Result;
use crate::learning::nlg::context::Context;
use std::collections::HashMap;

pub struct MarkdownFormatter {
    /// Vocabulary reverse mapping (token_id → word)
    reverse_vocab: HashMap<usize, String>,
}

impl MarkdownFormatter {
    pub fn new() -> Self {
        Self {
            reverse_vocab: HashMap::new(),
        }
    }

    /// Format paragraphs as markdown.
    pub fn format_paragraphs(
        &mut self,
        paragraphs: &[Vec<usize>],
        context: &Context,
    ) -> Result<String> {
        // Build reverse vocabulary
        self.build_reverse_vocab(context);

        let mut output = String::new();

        // Format each paragraph
        for (idx, tokens) in paragraphs.iter().enumerate() {
            if idx > 0 {
                output.push_str("\n\n");
            }

            let text = self.tokens_to_text(tokens);
            output.push_str(&text);
        }

        // Add key points if multiple concepts
        if context.key_concepts.len() > 1 {
            output.push_str("\n\n## Key Points\n\n");
            for concept in context.key_concepts.iter().take(5) {
                output.push_str("- ");
                output.push_str(concept);
                output.push('\n');
            }
        }

        Ok(output)
    }

    /// Build reverse vocabulary mapping.
    fn build_reverse_vocab(&mut self, context: &Context) {
        self.reverse_vocab.clear();
        for (word, &token_id) in &context.vocabulary {
            self.reverse_vocab.insert(token_id, word.clone());
        }
    }

    /// Convert tokens to text.
    fn tokens_to_text(&self, tokens: &[usize]) -> String {
        tokens
            .iter()
            .filter_map(|&token_id| self.reverse_vocab.get(&token_id))
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Default for MarkdownFormatter {
    fn default() -> Self {
        Self::new()
    }
}

pub use crate::learning::nlg::context::ResponseFormat;
