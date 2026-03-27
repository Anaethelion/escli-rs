// Licensed to Elasticsearch B.V. under one or more contributor
// license agreements. See the NOTICE file distributed with
// this work for additional information regarding copyright
// ownership. Elasticsearch B.V. licenses this file to you under
// the Apache License, Version 2.0 (the "License"); you may
// not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use clap::{Command, CommandFactory, Parser, ValueEnum};
use elasticsearch::http::headers::{HeaderMap, HeaderValue, CONTENT_TYPE};
use elasticsearch::http::response::Response;
use elasticsearch::http::transport::Transport;
use elasticsearch::http::Method;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

const DEFAULT_BATCH_SIZE: usize = 500;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Format {
    Json,
    Ndjson,
}

#[derive(Parser, Debug)]
pub struct Load {
    #[arg(help = "Path to the file to load, or - to read from stdin (default when omitted)")]
    file: Option<PathBuf>,

    #[arg(
        short,
        long,
        help = "Target index name (required for JSON format)",
    )]
    index: Option<String>,

    #[arg(
        short,
        long,
        help = "Number of documents per bulk request",
        default_value_t = DEFAULT_BATCH_SIZE
    )]
    size: usize,

    #[arg(
        short,
        long,
        help = "Ingest pipeline to use"
    )]
    pipeline: Option<String>,

    #[arg(
        short,
        long,
        help = "Input format: json or ndjson (inferred from extension if omitted)",
        value_enum
    )]
    format: Option<Format>,
}

#[derive(Deserialize)]
struct BulkResponse {
    errors: bool,
    items: Vec<BulkItem>,
}

#[derive(Deserialize)]
struct BulkItem {
    #[serde(alias = "index", alias = "create", alias = "update", alias = "delete")]
    action: BulkActionResult,
}

#[derive(Deserialize)]
struct BulkActionResult {
    status: u16,
    #[serde(default)]
    error: Option<Value>,
}

impl Load {
    pub fn new_command() -> Command {
        Self::command()
            .name("load")
            .about("Load a JSON or NDJSON file into Elasticsearch via the bulk API.")
            .long_about(
                r#"
            Load documents from a file into Elasticsearch using the bulk API.
            Both formats are streamed line-by-line so arbitrarily large files
            can be ingested without loading them entirely into memory.

            Supported formats:
              - JSON:   One JSON document per line (JSON Lines). Each line is a
                        raw document; action metadata is added automatically.
                        Requires --index.
              - NDJSON: Already in bulk format (action + document line pairs),
                        as produced by the `dump` command. Sent as-is.

            The format is inferred from the file extension (.json or .jsonl
            for JSON Lines, .ndjson for bulk NDJSON) unless overridden with
            --format.

            Documents are batched into chunks (default 500) to avoid hitting
            the Elasticsearch HTTP request size limit.

            Example usage:
                escli utils load data.ndjson
                escli utils load docs.json --index my-index
                escli utils load docs.jsonl --index my-index --pipeline my-pipeline --size 1000
            "#,
            )
    }

    pub async fn execute(
        self,
        transport: Transport,
        timeout: Option<Duration>,
    ) -> Result<Response, elasticsearch::Error> {
        let t = timeout.unwrap_or(Duration::from_secs(60));

        let is_stdin = self.file.as_ref().map_or(true, |p| p.as_os_str() == "-");

        let format = self.format.unwrap_or_else(|| {
            if is_stdin {
                eprintln!("Warning: reading from stdin with no --format; assuming NDJSON. Use --format to override.");
                return Format::Ndjson;
            }
            match self.file.as_ref().unwrap().extension().and_then(|e| e.to_str()) {
                Some("ndjson") => Format::Ndjson,
                Some("json" | "jsonl") => Format::Json,
                other => {
                    eprintln!(
                        "Warning: unknown extension {:?}, assuming JSON Lines format. Use --format to override.",
                        other.unwrap_or("(none)")
                    );
                    Format::Json
                }
            }
        });

        let mut path = match &self.index {
            Some(idx) => format!("/{}/_bulk", idx),
            None => "/_bulk".to_string(),
        };

        if let Some(ref pipeline) = self.pipeline {
            path.push_str(&format!("?pipeline={}", pipeline));
        }

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/x-ndjson"));

        let input: Box<dyn AsyncRead + Unpin> = if is_stdin {
            Box::new(tokio::io::stdin())
        } else {
            let file_path = self.file.as_ref().unwrap();
            Box::new(fs::File::open(file_path).await.map_err(|e| {
                eprintln!("Failed to open file {:?}: {}", file_path, e);
                e
            })?)
        };
        let mut reader = BufReader::new(input);

        let (total_indexed, total_errors, total_batches, total_http_errors) = match format {
            Format::Json => {
                self.load_json(&mut reader, &transport, &path, &headers, t).await?
            }
            Format::Ndjson => {
                self.load_ndjson(&mut reader, &transport, &path, &headers, t).await?
            }
        };

        eprintln!(
            "Done: {} documents indexed, {} errors across {} batch(es)",
            total_indexed, total_errors, total_batches
        );

        let status = if total_errors > 0 || total_http_errors > 0 { 400u16 } else { 200u16 };
        let hr = http::response::Builder::new()
            .status(status)
            .body(Vec::new())
            .unwrap();
        let rr = reqwest::Response::from(hr);
        Ok(Response::new(rr, elasticsearch::http::Method::Get))
    }

    /// JSON Lines format: one raw JSON document per line. Streamed
    /// line-by-line so arbitrarily large files can be ingested.
    async fn load_json(
        &self,
        reader: &mut (impl AsyncBufReadExt + Unpin),
        transport: &Transport,
        path: &str,
        headers: &HeaderMap,
        timeout: Duration,
    ) -> Result<(usize, usize, usize, usize), elasticsearch::Error> {
        let index = self.index.as_deref().unwrap_or_else(|| {
            eprintln!("Error: --index is required for JSON format");
            std::process::exit(1);
        });

        let action_line = serde_json::to_string(&serde_json::json!({ "index": { "_index": index } })).unwrap();

        let mut lines = reader.lines();

        let mut total_indexed: usize = 0;
        let mut total_errors: usize = 0;
        let mut total_http_errors: usize = 0;
        let mut batch_num: usize = 0;
        let mut body = String::new();
        let mut doc_count: usize = 0;

        while let Some(line) = lines.next_line().await.map_err(|e| {
            eprintln!("Failed to read line: {}", e);
            e
        })? {
            if line.is_empty() {
                continue;
            }
            body.push_str(&action_line);
            body.push('\n');
            body.push_str(&line);
            body.push('\n');
            doc_count += 1;

            if doc_count >= self.size {
                batch_num += 1;
                let (ok, err, http_fail) = send_bulk_batch(transport, path, headers, &body, batch_num, timeout).await?;
                total_indexed += ok;
                total_errors += err;
                if http_fail { total_http_errors += 1; }
                body.clear();
                doc_count = 0;
            }
        }

        if !body.is_empty() {
            batch_num += 1;
            let (ok, err, http_fail) = send_bulk_batch(transport, path, headers, &body, batch_num, timeout).await?;
            total_indexed += ok;
            total_errors += err;
            if http_fail { total_http_errors += 1; }
        }

        Ok((total_indexed, total_errors, batch_num, total_http_errors))
    }

    /// NDJSON format streams the file line-by-line, so it can handle
    /// arbitrarily large files without loading them entirely into memory.
    async fn load_ndjson(
        &self,
        reader: &mut (impl AsyncBufReadExt + Unpin),
        transport: &Transport,
        path: &str,
        headers: &HeaderMap,
        timeout: Duration,
    ) -> Result<(usize, usize, usize, usize), elasticsearch::Error> {
        let mut lines = reader.lines();

        let lines_per_batch = self.size * 2;
        let mut total_indexed: usize = 0;
        let mut total_errors: usize = 0;
        let mut total_http_errors: usize = 0;
        let mut batch_num: usize = 0;
        let mut body = String::new();
        let mut line_count: usize = 0;

        while let Some(line) = lines.next_line().await.map_err(|e| {
            eprintln!("Failed to read line: {}", e);
            e
        })? {
            if line.is_empty() {
                continue;
            }
            body.push_str(&line);
            body.push('\n');
            line_count += 1;

            if line_count >= lines_per_batch {
                batch_num += 1;
                let (ok, err, http_fail) = send_bulk_batch(transport, path, headers, &body, batch_num, timeout).await?;
                total_indexed += ok;
                total_errors += err;
                if http_fail { total_http_errors += 1; }
                body.clear();
                line_count = 0;
            }
        }

        if !body.is_empty() {
            batch_num += 1;
            let (ok, err, http_fail) = send_bulk_batch(transport, path, headers, &body, batch_num, timeout).await?;
            total_indexed += ok;
            total_errors += err;
            if http_fail { total_http_errors += 1; }
        }

        Ok((total_indexed, total_errors, batch_num, total_http_errors))
    }
}

/// Returns `(indexed, doc_errors, http_failed)` where `http_failed` is true
/// when the bulk endpoint itself returned a non-2xx status.
async fn send_bulk_batch(
    transport: &Transport,
    path: &str,
    headers: &HeaderMap,
    body: &str,
    batch_num: usize,
    timeout: Duration,
) -> Result<(usize, usize, bool), elasticsearch::Error> {
    let response: Response = transport
        .send(
            Method::Post,
            path,
            headers.clone(),
            Option::<&()>::None,
            Some(body),
            Some(timeout),
        )
        .await?;

    if !response.status_code().is_success() {
        let status = response.status_code();
        let text = response.text().await.unwrap_or_default();
        eprintln!(
            "Batch {}: bulk request failed with status {} - {}",
            batch_num, status, text
        );
        return Ok((0, 0, true));
    }

    let bulk_resp: BulkResponse = response.json().await?;
    let batch_errors: usize = bulk_resp
        .items
        .iter()
        .filter(|item| item.action.status >= 400)
        .count();
    let batch_ok = bulk_resp.items.len() - batch_errors;

    if bulk_resp.errors {
        for item in &bulk_resp.items {
            if let Some(ref err) = item.action.error {
                eprintln!("  Error: {}", err);
            }
        }
    }

    eprintln!(
        "Batch {}: {} indexed, {} errors",
        batch_num, batch_ok, batch_errors
    );

    Ok((batch_ok, batch_errors, false))
}

#[cfg(test)]
mod tests {
    /// Simulates the JSON Lines batching logic: one raw doc per line,
    /// action metadata prepended, batched by doc count.
    fn build_json_batches(contents: &str, index: &str, size: usize) -> Vec<String> {
        let action_line =
            serde_json::to_string(&serde_json::json!({ "index": { "_index": index } })).unwrap();
        let mut batches = Vec::new();
        let mut body = String::new();
        let mut doc_count: usize = 0;

        for line in contents.lines() {
            if line.is_empty() {
                continue;
            }
            body.push_str(&action_line);
            body.push('\n');
            body.push_str(line);
            body.push('\n');
            doc_count += 1;

            if doc_count >= size {
                batches.push(body.clone());
                body.clear();
                doc_count = 0;
            }
        }

        if !body.is_empty() {
            batches.push(body);
        }

        batches
    }

    /// Simulates the NDJSON batching logic: action+doc pairs already present,
    /// batched by line-pair count.
    fn build_ndjson_batches(contents: &str, size: usize) -> Vec<String> {
        let lines_per_batch = size * 2;
        let mut batches = Vec::new();
        let mut body = String::new();
        let mut line_count: usize = 0;

        for line in contents.lines() {
            if line.is_empty() {
                continue;
            }
            body.push_str(line);
            body.push('\n');
            line_count += 1;

            if line_count >= lines_per_batch {
                batches.push(body.clone());
                body.clear();
                line_count = 0;
            }
        }

        if !body.is_empty() {
            batches.push(body);
        }

        batches
    }

    #[test]
    fn test_json_lines_batching() {
        let contents = r#"{"field":"value1"}
{"field":"value2"}
{"field":"value3"}
"#;
        let batches = build_json_batches(contents, "test-index", 2);

        assert_eq!(batches.len(), 2);

        let lines: Vec<&str> = batches[0].lines().collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], r#"{"index":{"_index":"test-index"}}"#);
        assert_eq!(lines[1], r#"{"field":"value1"}"#);
        assert_eq!(lines[2], r#"{"index":{"_index":"test-index"}}"#);
        assert_eq!(lines[3], r#"{"field":"value2"}"#);

        let lines2: Vec<&str> = batches[1].lines().collect();
        assert_eq!(lines2.len(), 2);
        assert_eq!(lines2[0], r#"{"index":{"_index":"test-index"}}"#);
        assert_eq!(lines2[1], r#"{"field":"value3"}"#);
    }

    #[test]
    fn test_json_lines_skips_empty_lines() {
        let contents = "
{\"a\":1}

{\"a\":2}

";
        let batches = build_json_batches(contents, "idx", 100);
        assert_eq!(batches.len(), 1);
        let lines: Vec<&str> = batches[0].lines().collect();
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_ndjson_batching() {
        let contents = r#"{"index":{"_index":"idx"}}
{"field":"value1"}
{"index":{"_index":"idx"}}
{"field":"value2"}
{"index":{"_index":"idx"}}
{"field":"value3"}
"#;

        let batches = build_ndjson_batches(contents, 2);
        assert_eq!(batches.len(), 2);

        let lines1: Vec<&str> = batches[0].lines().collect();
        assert_eq!(lines1.len(), 4);

        let lines2: Vec<&str> = batches[1].lines().collect();
        assert_eq!(lines2.len(), 2);
    }

    #[test]
    fn test_ndjson_batching_empty() {
        let batches = build_ndjson_batches("", 100);
        assert!(batches.is_empty());
    }
}
