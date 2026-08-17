use super::*;
use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

impl Serialize for SemanticMatrix {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let triplets = self.to_triplets().map_err(serde::ser::Error::custom)?;

        let mut state = serializer.serialize_struct("SemanticMatrix", 11)?;
        state.serialize_field("matrix_id", &self.matrix_id)?;
        state.serialize_field("domain_label", &self.domain_label)?;
        state.serialize_field("triplets", &triplets)?;
        state.serialize_field("vocabulary_size", &self.vocabulary_size)?;
        state.serialize_field("update_count", &self.update_count)?;
        state.serialize_field("consolidation_threshold", &self.consolidation_threshold)?;
        state.serialize_field("coherence_score", &self.coherence_score)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.serialize_field("updated_at", &self.updated_at)?;
        state.serialize_field("confidence_history", &self.confidence_history)?;
        state.serialize_field("query_count", &self.query_count)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for SemanticMatrix {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            MatrixId,
            DomainLabel,
            Triplets,
            VocabularySize,
            UpdateCount,
            ConsolidationThreshold,
            CoherenceScore,
            CreatedAt,
            UpdatedAt,
            ConfidenceHistory,
            QueryCount,
        }

        struct SemanticMatrixVisitor;

        impl<'de> Visitor<'de> for SemanticMatrixVisitor {
            type Value = SemanticMatrix;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct SemanticMatrix")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SemanticMatrix, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut matrix_id = None;
                let mut domain_label = None;
                let mut triplets = None;
                let mut vocabulary_size = None;
                let mut update_count = None;
                let mut consolidation_threshold = None;
                let mut coherence_score = None;
                let mut created_at = None;
                let mut updated_at = None;
                let mut confidence_history = None;
                let mut query_count = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::MatrixId => {
                            matrix_id = Some(map.next_value()?);
                        }
                        Field::DomainLabel => {
                            domain_label = Some(map.next_value()?);
                        }
                        Field::Triplets => {
                            triplets = Some(map.next_value()?);
                        }
                        Field::VocabularySize => {
                            vocabulary_size = Some(map.next_value()?);
                        }
                        Field::UpdateCount => {
                            update_count = Some(map.next_value()?);
                        }
                        Field::ConsolidationThreshold => {
                            consolidation_threshold = Some(map.next_value()?);
                        }
                        Field::CoherenceScore => {
                            coherence_score = Some(map.next_value()?);
                        }
                        Field::CreatedAt => {
                            created_at = Some(map.next_value()?);
                        }
                        Field::UpdatedAt => {
                            updated_at = Some(map.next_value()?);
                        }
                        Field::ConfidenceHistory => {
                            confidence_history = Some(map.next_value()?);
                        }
                        Field::QueryCount => {
                            query_count = Some(map.next_value()?);
                        }
                    }
                }

                let matrix_id = matrix_id.ok_or_else(|| de::Error::missing_field("matrix_id"))?;
                let domain_label =
                    domain_label.ok_or_else(|| de::Error::missing_field("domain_label"))?;
                let triplets: Vec<(usize, usize, f64)> =
                    triplets.ok_or_else(|| de::Error::missing_field("triplets"))?;
                let vocabulary_size =
                    vocabulary_size.ok_or_else(|| de::Error::missing_field("vocabulary_size"))?;
                let update_count =
                    update_count.ok_or_else(|| de::Error::missing_field("update_count"))?;
                let consolidation_threshold = consolidation_threshold
                    .ok_or_else(|| de::Error::missing_field("consolidation_threshold"))?;
                let coherence_score =
                    coherence_score.ok_or_else(|| de::Error::missing_field("coherence_score"))?;
                let created_at =
                    created_at.ok_or_else(|| de::Error::missing_field("created_at"))?;
                let updated_at =
                    updated_at.ok_or_else(|| de::Error::missing_field("updated_at"))?;
                let confidence_history = confidence_history
                    .ok_or_else(|| de::Error::missing_field("confidence_history"))?;
                let query_count =
                    query_count.ok_or_else(|| de::Error::missing_field("query_count"))?;

                let mut updates = TriMat::new((vocabulary_size, vocabulary_size));
                for (row, col, value) in triplets {
                    updates.add_triplet(row, col, value);
                }

                Ok(SemanticMatrix {
                    matrix_id,
                    domain_label,
                    updates,
                    compressed: None,
                    vocabulary_size,
                    update_count,
                    consolidation_threshold,
                    coherence_score,
                    created_at,
                    updated_at,
                    confidence_history,
                    query_count,
                    cached_eigen: None,
                    eigen_cache_update_count: 0,
                })
            }
        }

        const FIELDS: &[&str] = &[
            "matrix_id",
            "domain_label",
            "triplets",
            "vocabulary_size",
            "update_count",
            "consolidation_threshold",
            "coherence_score",
            "created_at",
            "updated_at",
            "confidence_history",
            "query_count",
        ];
        deserializer.deserialize_struct("SemanticMatrix", FIELDS, SemanticMatrixVisitor)
    }
}
