#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSearchHit {
    pub session_id: String,
    pub record_id: String,
    pub snippet: String,
}
