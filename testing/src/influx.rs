use anyhow::{Context, anyhow};
use serde_json::Value;
use std::time::{Duration, Instant};

const DEFAULT_DB: &str = "otel";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone, Debug)]
pub struct InfluxClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
    db: String,
    timeout: Duration,
    poll_interval: Duration,
}

impl InfluxClient {
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: token.into(),
            db: DEFAULT_DB.to_string(),
            timeout: DEFAULT_TIMEOUT,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }

    pub fn with_db(mut self, db: impl Into<String>) -> Self {
        self.db = db.into();
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    /// Single `POST /api/v3/query_sql`. Errors on transport failure or non-2xx.
    pub async fn query(&self, sql: &str) -> anyhow::Result<Vec<Value>> {
        let url = format!("{}/api/v3/query_sql", self.base_url);
        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(&serde_json::json!({ "db": self.db, "q": sql }))
            .send()
            .await
            .with_context(|| format!("POST {url}"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .with_context(|| format!("reading body from {url}"))?;
        if !status.is_success() {
            return Err(anyhow!("InfluxDB status={status} body={body} sql={sql}"));
        }

        serde_json::from_str::<Value>(&body)
            .with_context(|| format!("parsing InfluxDB JSON: {body}"))?
            .as_array()
            .cloned()
            .ok_or_else(|| anyhow!("InfluxDB returned a non-array body: {body}"))
    }

    /// Poll until `sql` returns at least one row.
    pub async fn wait_for_rows(&self, sql: &str) -> anyhow::Result<Vec<Value>> {
        self.wait_for(sql, |_| true).await
    }

    /// Poll until at least one returned row satisfies `predicate`. InfluxDB
    /// creates databases and tables lazily, so every failure — transport,
    /// non-2xx, or empty result — is retried until the deadline.
    pub async fn wait_for<F>(&self, sql: &str, predicate: F) -> anyhow::Result<Vec<Value>>
    where
        F: Fn(&Value) -> bool,
    {
        let deadline = Instant::now() + self.timeout;
        let mut last_error;
        loop {
            match self.query(sql).await {
                Ok(rows) if rows.iter().any(&predicate) => return Ok(rows),
                Ok(_) => last_error = format!("no row matched, sql={sql}"),
                Err(e) => last_error = format!("{e:#}"),
            }
            if Instant::now() >= deadline {
                return Err(anyhow!(
                    "InfluxDB wait timed out after {:?}: {last_error}",
                    self.timeout
                ));
            }
            tokio::time::sleep(self.poll_interval).await;
        }
    }
}
