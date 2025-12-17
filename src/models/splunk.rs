use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchJob {
    pub sid: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JobStatus {
    #[serde(rename = "isDone")]
    pub is_done: bool,
    #[serde(rename = "dispatchState")]
    pub dispatch_state: String,
    #[serde(rename = "resultCount", default)]
    pub result_count: u64,
    #[serde(rename = "runDuration", default)]
    pub run_duration: f64,
    #[serde(rename = "scanCount", default)]
    pub scan_count: u64,
    #[serde(rename = "eventCount", default)]
    pub event_count: u64,
    #[serde(rename = "doneProgress", default)]
    pub done_progress: Option<f64>,
    #[serde(rename = "messages", default)]
    pub messages: Vec<serde_json::Value>,
}


