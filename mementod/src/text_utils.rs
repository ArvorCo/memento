use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryMode {
    DocumentLookup,
    EpisodicRecall,
    ConceptSearch,
}

pub fn tokenize_text(text: &str) -> Vec<String> {
    text.split_whitespace()
        .flat_map(|segment| segment.split(|c: char| !c.is_alphanumeric()))
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

pub fn ascii_fold(text: &str) -> String {
    text.nfd()
        .filter(|ch| !is_combining_mark(*ch))
        .collect::<String>()
        .to_lowercase()
}

pub fn tokenize_folded_text(text: &str) -> Vec<String> {
    tokenize_text(&ascii_fold(text))
}

pub fn is_low_signal_query_term(term: &str) -> bool {
    matches!(
        term,
        "what"
            | "which"
            | "who"
            | "when"
            | "where"
            | "how"
            | "did"
            | "does"
            | "was"
            | "were"
            | "are"
            | "is"
            | "we"
            | "record"
            | "about"
            | "say"
            | "the"
            | "and"
            | "for"
            | "with"
            | "from"
            | "that"
            | "this"
            | "into"
            | "have"
            | "has"
            | "our"
            | "its"
            | "para"
            | "com"
            | "sem"
            | "uma"
            | "um"
            | "que"
            | "dos"
            | "das"
            | "por"
            | "qual"
            | "quais"
            | "quem"
            | "quando"
            | "onde"
            | "como"
            | "era"
            | "eram"
            | "foi"
            | "foram"
            | "esta"
            | "estao"
            | "aconteceu"
            | "do"
            | "da"
            | "no"
            | "na"
            | "nos"
            | "nas"
            | "ao"
            | "aos"
            | "em"
            | "se"
            | "os"
            | "as"
            | "o"
            | "a"
            | "e"
    )
}

pub fn has_recall_intent(query: &str) -> bool {
    let query = ascii_fold(query);
    [
        "what did we record",
        "what did we",
        "o que decidimos",
        "relembre",
        "lembramos",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub fn detect_query_mode(query: &str) -> QueryMode {
    let folded = ascii_fold(query);
    if folded.contains("what does")
        || folded.contains(" say")
        || folded.contains(" say?")
        || folded.contains(" file")
        || folded.contains(" files")
        || folded.contains(" path")
        || folded.contains(" paths")
        || folded.contains("where is")
        || folded.contains("where are")
        || folded.contains("which file")
        || folded.contains("which files")
        || folded.contains("which doc")
        || folded.contains("profile")
        || folded.contains("catalog")
        || folded.contains("catalogo")
    {
        QueryMode::DocumentLookup
    } else if has_recall_intent(query) {
        QueryMode::EpisodicRecall
    } else {
        QueryMode::ConceptSearch
    }
}

pub fn query_has_any_term(query_terms: &[String], candidates: &[&str]) -> bool {
    query_terms
        .iter()
        .any(|term| candidates.iter().any(|candidate| term == candidate))
}

pub fn query_exactness_terms(query: &str) -> Vec<String> {
    tokenize_folded_text(query)
        .into_iter()
        .filter(|term| term.len() >= 2 && !is_low_signal_query_term(term))
        .collect()
}

/// Small, deterministic cross-language bridges for high-value retrieval terms.
///
/// These are query-time alternatives, not replacements: the lexical ranker treats
/// every returned term as one disjunction, so translations and inflections cannot
/// inflate a document merely by repeating the same concept in several forms.
pub fn lexical_query_alternatives(term: &str) -> Vec<String> {
    let alternatives: &[&str] = match term {
        "plano" | "plan" | "planejamento" | "planning" => {
            &["plano", "plan", "planejamento", "planning"]
        }
        "converter" | "conversao" | "convert" | "conversion" | "converting" => &[
            "converter",
            "conversao",
            "convert",
            "conversion",
            "converting",
        ],
        "cliente" | "clientes" | "client" | "clients" | "customer" | "customers" => &[
            "cliente",
            "clientes",
            "client",
            "clients",
            "customer",
            "customers",
        ],
        "pagante" | "pagantes" | "paying" | "paid" | "payer" | "payers" => {
            &["pagante", "pagantes", "paying", "paid", "payer", "payers"]
        }
        "venda" | "vendas" | "sale" | "sales" => &["venda", "vendas", "sale", "sales"],
        "decisao" | "decisoes" | "decision" | "decisions" => {
            &["decisao", "decisoes", "decision", "decisions"]
        }
        "lancamento" | "launch" | "launching" => &["lancamento", "launch", "launching"],
        "juridico" | "juridica" | "legal" => &["juridico", "juridica", "legal"],
        "advogado" | "advogados" | "lawyer" | "lawyers" | "attorney" | "attorneys" => &[
            "advogado",
            "advogados",
            "lawyer",
            "lawyers",
            "attorney",
            "attorneys",
        ],
        "prioridade" | "prioridades" | "priority" | "priorities" => {
            &["prioridade", "prioridades", "priority", "priorities"]
        }
        _ => return vec![term.to_string()],
    };
    alternatives
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

pub fn strip_date_prefix(text: &str) -> String {
    let parts = text.split('-').collect::<Vec<_>>();
    if parts.len() >= 4
        && parts[0].len() == 4
        && parts[1].len() <= 2
        && parts[2].len() <= 2
        && parts[0].chars().all(|ch| ch.is_ascii_digit())
        && parts[1].chars().all(|ch| ch.is_ascii_digit())
        && parts[2].chars().all(|ch| ch.is_ascii_digit())
    {
        parts[3..].join(" ")
    } else {
        text.replace(['-', '_'], " ")
    }
}

pub fn parse_date_tokens(text: &str) -> Option<(u32, u32, u32)> {
    let numbers = tokenize_text(text)
        .into_iter()
        .filter_map(|term| term.parse::<u32>().ok())
        .collect::<Vec<_>>();

    for window in numbers.windows(3) {
        let [year, month, day] = match window {
            [year, month, day] => [*year, *month, *day],
            _ => continue,
        };
        if (1900..=2100).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day) {
            return Some((year, month, day));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_intent_requires_explicit_memory_language() {
        assert!(has_recall_intent("relembre o que decidimos no sprint"));
        assert!(!has_recall_intent("o que aconteceu no sprint ontem?"));
        assert!(!has_recall_intent("como era o plano de vendas?"));
    }

    #[test]
    fn catalog_queries_target_canonical_documents() {
        assert_eq!(
            detect_query_mode("what are Atlas's product priorities and product catalog?"),
            QueryMode::DocumentLookup
        );
    }

    #[test]
    fn portuguese_question_grammar_is_low_signal() {
        for term in ["como", "era", "quais", "foram", "no", "do", "em"] {
            assert!(is_low_signal_query_term(term), "{term}");
        }
        assert!(!is_low_signal_query_term("memento"));
    }

    #[test]
    fn lexical_bridges_cover_inflection_and_translation() {
        let alternatives = lexical_query_alternatives("clientes");
        assert!(alternatives.iter().any(|term| term == "clientes"));
        assert!(alternatives.iter().any(|term| term == "customers"));
    }
}
