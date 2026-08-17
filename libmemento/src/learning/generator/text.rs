pub(super) fn tokenize_terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .filter(|token| token.len() >= 4)
        .filter(|token| {
            !matches!(
                token.as_str(),
                "this"
                    | "that"
                    | "with"
                    | "from"
                    | "have"
                    | "were"
                    | "what"
                    | "about"
                    | "para"
                    | "como"
                    | "qual"
                    | "quais"
                    | "sobre"
                    | "porque"
                    | "where"
                    | "when"
                    | "plan"
                    | "note"
                    | "notes"
                    | "they"
                    | "them"
                    | "into"
                    | "your"
                    | "will"
                    | "more"
                    | "http"
                    | "https"
                    | "localhost"
                    | "source"
                    | "startline"
                    | "score"
                    | "lines"
            )
        })
        .collect()
}

pub(super) fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut table_headers: Option<Vec<String>> = None;
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(cells) = markdown_table_cells(line) {
            if cells
                .iter()
                .all(|cell| cell.chars().all(|ch| matches!(ch, '-' | ':')))
            {
                continue;
            }
            if let Some(headers) = table_headers
                .as_ref()
                .filter(|headers| headers.len() == cells.len())
            {
                sentences.push(
                    headers
                        .iter()
                        .zip(cells)
                        .map(|(header, value)| format!("{}: {}", header.replace('_', " "), value))
                        .collect::<Vec<_>>()
                        .join("; "),
                );
            } else {
                table_headers = Some(cells);
            }
            continue;
        }
        table_headers = None;
        sentences.extend(split_prose_line(line));
    }
    sentences
}

fn markdown_table_cells(line: &str) -> Option<Vec<String>> {
    if !line.starts_with('|') || !line.ends_with('|') {
        return None;
    }
    let cells = line
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    (!cells.is_empty() && cells.iter().all(|cell| !cell.is_empty())).then_some(cells)
}

fn split_prose_line(line: &str) -> Vec<String> {
    let characters = line.chars().collect::<Vec<_>>();
    let mut sentences = Vec::new();
    let mut current = String::new();
    for (index, character) in characters.iter().copied().enumerate() {
        current.push(character);
        let decimal_point = character == '.'
            && index > 0
            && index + 1 < characters.len()
            && characters[index - 1].is_ascii_digit()
            && characters[index + 1].is_ascii_digit();
        let boundary = matches!(character, '.' | '!' | '?')
            && !decimal_point
            && characters
                .get(index + 1)
                .is_none_or(|next| next.is_whitespace());
        if boundary {
            let value = current.trim();
            if !value.is_empty() {
                sentences.push(value.to_string());
            }
            current.clear();
        }
    }
    let value = current.trim();
    if !value.is_empty() {
        sentences.push(value.to_string());
    }
    sentences
}

pub(super) fn first_meaningful_sentence(text: &str) -> Option<String> {
    split_sentences(text)
        .into_iter()
        .find(|sentence| sentence.len() >= 24)
        .map(|sentence| clean_sentence(&sentence))
}

pub(super) fn clean_sentence(sentence: &str) -> String {
    let trimmed = sentence
        .trim()
        .trim_start_matches(['#', '-', '*', '>', ' '])
        .trim()
        .trim_matches('`')
        .trim_matches('"');
    if trimmed.ends_with('.') {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    }
}

pub(super) fn normalize_sentence_key(sentence: &str) -> String {
    sentence
        .chars()
        .flat_map(|ch| ch.to_lowercase())
        .filter(|ch| ch.is_alphanumeric() || ch.is_whitespace())
        .collect::<String>()
}
