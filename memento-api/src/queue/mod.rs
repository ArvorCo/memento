/// Import queue — Phase 2
///
/// Will implement:
/// - Google Cloud Pub/Sub producer for import jobs
/// - Kafka migration when revenue > $500/mo
/// - Dead letter queue handling
#[allow(dead_code)]
pub struct ImportJob {
    pub user_id: String,
    pub source_type: String,
    pub payload: Vec<u8>,
}
