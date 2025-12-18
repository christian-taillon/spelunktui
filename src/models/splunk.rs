use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchJob {
    pub sid: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JobStatus {
    #[serde(rename = "isDone")]
    pub is_done: bool,
    #[serde(rename = "dispatchState")]
    pub dispatch_state: String,
    #[serde(rename = "resultCount")]
    pub result_count: u64,
    #[serde(rename = "runDuration")]
    pub run_duration: f64,
    #[serde(rename = "scanCount")]
    pub scan_count: u64,
    #[serde(rename = "eventCount")]
    pub event_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SplunkError {
    pub messages: Vec<SplunkErrorMessage>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SplunkErrorMessage {
    pub r#type: String,
    pub text: String,
}
