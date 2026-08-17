use std::path::Path;

pub mod vault;

pub use vault::{VaultSource, EXCLUDED_DIRS, PROTECTED_PATTERNS};

pub(crate) fn normalize_relative_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) fn wildcard_match(pattern: &str, value: &str) -> bool {
    wildcard_match_bytes(pattern.as_bytes(), value.as_bytes())
}

fn wildcard_match_bytes(pattern: &[u8], value: &[u8]) -> bool {
    let (mut pattern_index, mut value_index) = (0, 0);
    let (mut star_index, mut match_index) = (None, 0);

    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == value[value_index]
                || pattern[pattern_index] == b'?'
                || char_class_matches(pattern, &mut pattern_index, value[value_index]))
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            match_index = value_index;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            match_index += 1;
            value_index = match_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

fn char_class_matches(pattern: &[u8], pattern_index: &mut usize, value: u8) -> bool {
    if pattern[*pattern_index] != b'[' {
        return false;
    }

    let mut cursor = *pattern_index + 1;
    let mut matched = false;

    while cursor < pattern.len() && pattern[cursor] != b']' {
        if cursor + 2 < pattern.len() && pattern[cursor + 1] == b'-' && pattern[cursor + 2] != b']'
        {
            matched |= pattern[cursor] <= value && value <= pattern[cursor + 2];
            cursor += 3;
        } else {
            matched |= pattern[cursor] == value;
            cursor += 1;
        }
    }

    if cursor >= pattern.len() || pattern[cursor] != b']' {
        return false;
    }

    *pattern_index = cursor;
    matched
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_relative_path_converts_windows_separators() {
        assert_eq!(
            normalize_relative_path(Path::new("projects\\acme\\note.md")),
            "projects/acme/note.md"
        );
    }

    #[test]
    fn wildcard_match_handles_glob_style_patterns() {
        assert!(wildcard_match("_*_hub.md", "_acme_hub.md"));
        assert!(wildcard_match("MOC - *.md", "MOC - Artigos.md"));
        assert!(wildcard_match("issues/[0-9]*.md", "issues/42.md"));
        assert!(!wildcard_match("*Hub*.md", "projects/acme/readme.md"));
    }
}
