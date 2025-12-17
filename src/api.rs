use reqwest::Client;
use serde_json::Value;
use std::error::Error;
use log::error;
use crate::models::splunk::{SearchJob, JobStatus};

#[derive(Clone)]
pub struct SplunkClient {
    base_url: String,
    token: String,
    client: Client,
}

impl SplunkClient {
    pub fn new(base_url: String, token: String, verify_ssl: bool) -> Self {
        let client = Client::builder()
            .danger_accept_invalid_certs(!verify_ssl)
            .build()
            .expect("Failed to build HTTP client");

        // Ensure base_url doesn't end with slash for consistency
        let base_url = if base_url.ends_with('/') {
            base_url[..base_url.len()-1].to_string()
        } else {
            base_url
        };

        Self {
            base_url,
            token,
            client,
        }
    }

    pub async fn create_search(&self, query: &str) -> Result<String, Box<dyn Error>> {
        let url = format!("{}/services/search/jobs", self.base_url);
        
        // Ensure query starts with "search " or similar if required, but usually Splunk adds it or implicit.
        // Actually, often it's better to pass it as is.
        
        let params = [
            ("search", query),
            ("output_mode", "json"),
            ("exec_mode", "normal"),
        ];

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .form(&params)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            error!("Splunk API Error {}: {}", status, text);
            return Err(format!("API Error {}: {}", status, text).into());
        }

        let job: SearchJob = serde_json::from_str(&text)?;
        Ok(job.sid)
    }

    pub async fn get_job_status(&self, sid: &str) -> Result<JobStatus, Box<dyn Error>> {
        let url = format!("{}/services/search/jobs/{}", self.base_url, sid);
        
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .query(&[("output_mode", "json")])
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
             // Handle 404 specially?
             return Err(format!("API Error {}: {}", status, text).into());
        }

        // The JSON response for job status is nested in `entry` list usually in Splunk's Atom Feed over JSON
        // Wait, output_mode=json usually returns a specific structure.
        // Let's assume standard Splunk REST structure:
        // { "entry": [ { "content": { ... } } ] }

        let json: Value = serde_json::from_str(&text)?;

        if let Some(entry) = json.get("entry").and_then(|e| e.as_array()).and_then(|a| a.first()) {
            if let Some(content) = entry.get("content") {
                let job_status: JobStatus = serde_json::from_value(content.clone())?;
                return Ok(job_status);
            }
        }

        Err(format!("Failed to parse job status response: {}", text).into())
    }

    pub async fn get_results(&self, sid: &str, count: u32, offset: u32) -> Result<Vec<Value>, Box<dyn Error>> {
        let url = format!("{}/services/search/jobs/{}/results", self.base_url, sid);

        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .query(&[
                ("output_mode", "json"),
                ("count", &count.to_string()),
                ("offset", &offset.to_string())
            ])
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            return Err(format!("API Error {}: {}", status, text).into());
        }

        let json: Value = serde_json::from_str(&text)?;

        // Results are usually in "results" array
        if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
            return Ok(results.clone());
        }

        // If empty results or different structure
        Ok(vec![])
    }

    pub async fn delete_job(&self, sid: &str) -> Result<(), Box<dyn Error>> {
        let url = format!("{}/services/search/jobs/{}", self.base_url, sid);

        let response = self.client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await?;

        if !response.status().is_success() {
             return Err(format!("Failed to delete job: {}", response.status()).into());
        }

        Ok(())
    }

    pub fn get_shareable_url(&self, sid: &str) -> String {
        // Assume base_url is like https://splunk.example.com:8089 or https://splunk.example.com
        // We need the web interface URL, which usually runs on 8000, or same host different port.
        // Or we just use the base URL and hope it's the web URL or the user provided the web URL.
        // Usually, the management port is 8089. The web port is 8000.
        // Let's assume the user configures SPLUNK_BASE_URL as the management URL.
        // If it contains :8089, we might want to strip it or replace with 8000 for the link,
        // but that's a guess. Best effort: use the host.

        // A robust way is to ask the user for WEB_URL, but we didn't add that to config.
        // We will try to replace :8089 with :8000 if present, otherwise append path.
        
        let web_url = self.base_url.replace(":8089", ":8000");
        format!("{}/en-US/app/search/search?sid={}", web_url, sid)
    }
}
