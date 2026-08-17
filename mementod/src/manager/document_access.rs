use super::*;

const MAX_DOCUMENT_PAGE_CHARS: usize = 20_000;

impl MementoManager {
    pub async fn get_document(
        &self,
        request: &DocumentRequest,
    ) -> Result<Option<DocumentResponse>> {
        let max_chars = request.max_chars.clamp(1, MAX_DOCUMENT_PAGE_CHARS);
        let state = self.state.read().await;
        let Some(document) = state
            .documents
            .iter()
            .find(|document| document.source_path == request.source_path)
        else {
            return Ok(None);
        };
        let total_chars = document.canonical_text.chars().count();
        let offset_chars = request.offset_chars.min(total_chars);
        let content = document
            .canonical_text
            .chars()
            .skip(offset_chars)
            .take(max_chars)
            .collect::<String>();
        let returned_chars = content.chars().count();
        let next = offset_chars.saturating_add(returned_chars);
        let has_more = next < total_chars;
        Ok(Some(DocumentResponse {
            source_path: document.source_path.clone(),
            title: document.title.clone(),
            content,
            offset_chars,
            returned_chars,
            total_chars,
            has_more,
            next_offset_chars: has_more.then_some(next),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn document_access_pages_unicode_by_character() {
        let temp = tempfile::tempdir().unwrap();
        let manager = MementoManager::new(temp.path()).unwrap();
        manager
            .import(&ImportRequest {
                source: "file".to_string(),
                path: Some({
                    let path = temp.path().join("unicode.md");
                    fs::write(&path, "# Ação\náβcdef").unwrap();
                    path.to_string_lossy().to_string()
                }),
            })
            .await
            .unwrap();
        let source_path = normalize_path(temp.path().join("unicode.md").to_str().unwrap())
            .to_string_lossy()
            .to_string();

        let response = manager
            .get_document(&DocumentRequest {
                source_path,
                offset_chars: 7,
                max_chars: 3,
            })
            .await
            .unwrap()
            .unwrap();

        assert_eq!(response.content, "áβc");
        assert!(response.has_more);
        assert_eq!(response.next_offset_chars, Some(10));
    }
}
