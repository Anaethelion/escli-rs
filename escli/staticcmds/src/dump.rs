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

use clap::{Command, CommandFactory, Parser};
use elasticsearch::http::response::Response;
use elasticsearch::http::transport::Transport;
use elasticsearch::{Elasticsearch, OpenPointInTimeParts, SearchParts};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::fs::{File, OpenOptions};
use tokio::io::Stdout;
use tokio::io::{AsyncWrite, AsyncWriteExt};

#[derive(Parser, Debug)]
pub struct Dump {
    #[arg(
        required = true,
        value_delimiter = ',',
        help = "List of indices to dump, comma separated"
    )]
    indices: Vec<String>,

    #[arg(
        short,
        long,
        help = "Size of each batch to dump, default is 500",
        default_value_t = 500
    )]
    size: usize,

    #[arg(
        short,
        long,
        help = "Timeout for the operation, default is 1 minute",
        default_value = "1m"
    )]
    keep_alive: String,

    #[arg(short, long, help = "Output file location, default is stdout")]
    output: Option<PathBuf>,
}

#[derive(Deserialize, Debug)]
struct PontInTime {
    id: String,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum PointInTimeVariant {
    Success(PontInTime),
    Error(Box<ElasticsearchError>),
}

#[derive(Deserialize, Debug)]
struct SearchResult {
    pit_id: String,
    hits: Hits,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum SearchResultsVariant {
    Success(SearchResult),
    Error(Box<ElasticsearchError>),
}

#[derive(Deserialize, Debug)]
struct Hits {
    hits: Vec<Hit>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Hit {
    _source: Value,
    sort: Vec<u64>,
}

enum Output {
    File(File),
    Stdout(Stdout),
}

impl AsyncWrite for Output {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        let this = self.get_mut();
        match this {
            Output::File(f) => Pin::new(f).poll_write(cx, buf),
            Output::Stdout(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let this = self.get_mut();
        match this {
            Output::File(f) => Pin::new(f).poll_flush(cx),
            Output::Stdout(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let this = self.get_mut();
        match this {
            Output::File(f) => Pin::new(f).poll_shutdown(cx),
            Output::Stdout(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct CausedBy {
    r#type: String,
    reason: String,
    caused_by: RootCause,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct FailedShard {
    shard: i64,
    index: String,
    node: String,
    reason: RootCause,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct RootCause {
    r#type: String,
    reason: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct EsError {
    root_cause: Vec<RootCause>,
    r#type: String,
    reason: String,
    phase: String,
    grouped: bool,
    failed_shards: Vec<FailedShard>,
    caused_by: CausedBy,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct ElasticsearchError {
    error: EsError,
    status: i64,
}

impl Dump {
    pub fn new_command() -> Command {
        Self::command()
            .name("dump")
            .about("Dump one or more index as ndjson.")
            .long_about(
                r#"
            This command dumps the contents of one or more indices in ndjson format.
            Each document is prefixed with an action line for bulk operations.
            The action line is in the format:
            { "index": { "_index": "<index_name>" } }
            
            The documents are sorted by shard and document ID.
            The command uses point-in-time (PIT) to ensure consistent reads across the index.
            The PIT is kept alive for the duration of the operation.
            
            The command supports specifying a size for each batch of documents to be dumped.
            The default size is 500 documents per batch.

            The command also supports specifying a keep-alive duration for the PIT.
            The default keep-alive duration is 1 minute.

            Example usage:
                escli utils dump index1,index2 --size 1000 --keep-alive 5m
            "#,
            )
    }

    pub async fn execute(
        self,
        transport: Transport,
        timeout: Option<Duration>,
    ) -> Result<Response, elasticsearch::Error> {
        let client = Elasticsearch::new(transport);
        let indices: Vec<&str> = self.indices.iter().map(String::as_str).collect();
        let t = timeout.unwrap_or(Duration::from_secs(60));

        let mut output = match self.output {
            Some(ref path) => {
                let file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(path)
                    .await
                    .map_err(|e| {
                        eprintln!("Failed to open output file {:?}: {}", path, e);
                        e
                    })?;
                Output::File(file)
            }
            None => Output::Stdout(tokio::io::stdout()),
        };

        for index in indices {
            let pit_response = client
                .open_point_in_time(OpenPointInTimeParts::Index(&[index]))
                .keep_alive(&self.keep_alive)
                .request_timeout(t)
                .send()
                .await?;

            if pit_response.status_code() != http::StatusCode::OK {
                let status = pit_response.status_code();
                let body = pit_response.text().await.unwrap_or_default();
                eprintln!(
                    "Failed to open PIT for index '{}': {} - {}",
                    index, status, body
                );
                continue;
            }

            let initial_pit = match pit_response.json::<PointInTimeVariant>().await? {
                PointInTimeVariant::Success(pit) => pit,
                PointInTimeVariant::Error(err) => {
                    eprintln!("Error opening PIT for index '{}': {:?}", index, err);
                    continue;
                }
            };

            let initial_search = client
                .search(SearchParts::None)
                .body(json!({
                    "size": self.size,
                    "pit": { "id": initial_pit.id, "keep_alive": self.keep_alive },
                    "query": { "match_all": {} },
                    "sort": [{ "_shard_doc": { "order": "asc" } }]
                }))
                .send()
                .await?;

            let initial_documents = match initial_search.json::<SearchResultsVariant>().await? {
                SearchResultsVariant::Success(docs) => docs,
                SearchResultsVariant::Error(err) => {
                    eprintln!(
                        "Error during initial search for index '{}': {:?}",
                        index, err
                    );
                    continue;
                }
            };

            persist_ndjson(&initial_documents, index, &mut output).await?;

            let mut next_pit = initial_documents.pit_id;
            let mut next_search_after = initial_documents
                .hits
                .hits
                .last()
                .and_then(|hit| hit.sort.first())
                .copied();

            loop {
                let payload = json!({
                    "size": self.size,
                    "pit": { "id": next_pit, "keep_alive": self.keep_alive },
                    "query": { "match_all": {} },
                    "sort": [{ "_shard_doc": { "order": "asc" } }],
                    "search_after": next_search_after.map(|x| vec![x]).unwrap_or_default()
                });

                let search_response = client
                    .search(SearchParts::None)
                    .body(payload)
                    .send()
                    .await?;

                let documents: SearchResult =
                    match search_response.json::<SearchResultsVariant>().await? {
                        SearchResultsVariant::Success(docs) => docs,
                        SearchResultsVariant::Error(err) => {
                            eprintln!("Error during search after for index '{}': {:?}", index, err);
                            break;
                        }
                    };

                if documents.hits.hits.is_empty() {
                    break;
                } else {
                    persist_ndjson(&documents, index, &mut output).await?;
                }

                next_pit = documents.pit_id;
                next_search_after = documents
                    .hits
                    .hits
                    .last()
                    .and_then(|hit| hit.sort.first())
                    .copied();
            }
        }
        output.flush().await?;
        output.shutdown().await?;

        let hr = http::response::Response::new(Vec::new());
        let rr = reqwest::Response::from(hr);
        Ok(Response::new(rr, elasticsearch::http::Method::Get))
    }
}

/// Writes the search results to the specified output in NDJSON format.
///
/// # Arguments
///
/// * `result` - A reference to a `SearchResult` containing the documents to process.
/// * `index` - A string slice representing the name of the index being processed.
/// * `output` - A mutable reference to an object implementing the `Write` trait,
///   where the NDJSON data will be written.
///
/// # Returns
///
/// * `Result<(), Error>` - Returns `Ok(())` if the operation is successful, or an `Error` if an I/O error occurs.
///
/// # Errors
///
/// This function will return an error if writing to the output fails or if serializing
/// the document source to JSON fails.
///
async fn persist_ndjson(
    result: &SearchResult,
    index: &str,
    output: &mut (impl AsyncWrite + Unpin),
) -> Result<(), IoError> {
    for doc in result.hits.hits.iter() {
        let action_line = json!({ "index": { "_index": index } });

        let action_s =
            serde_json::to_string(&action_line).map_err(|e| IoError::new(IoErrorKind::Other, e))?;
        output.write_all(action_s.as_bytes()).await?;
        output.write_all(b"\n").await?;

        let doc_s =
            serde_json::to_string(&doc._source).map_err(|e| IoError::new(IoErrorKind::Other, e))?;
        output.write_all(doc_s.as_bytes()).await?;
        output.write_all(b"\n").await?;
    }
    output.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn create_sample_search_result() -> SearchResult {
        SearchResult {
            pit_id: "sample_pit_id".to_string(),
            hits: Hits {
                hits: vec![
                    Hit {
                        _source: json!({"field": "value1"}),
                        sort: vec![1],
                    },
                    Hit {
                        _source: json!({"field": "value2"}),
                        sort: vec![2],
                    },
                ],
            },
        }
    }

    #[tokio::test]
    async fn test_persist_ndjson() {
        let search_result = create_sample_search_result();
        let mut output = Cursor::new(Vec::new());
        persist_ndjson(&search_result, "test_index", &mut output).await.unwrap();
        let output_str = String::from_utf8(output.into_inner()).unwrap();
        let expected_output = r#"{"index":{"_index":"test_index"}}
{"field":"value1"}
{"index":{"_index":"test_index"}}
{"field":"value2"}
"#;
        assert_eq!(output_str, expected_output);
    }

    #[tokio::test]
    async fn test_persist_ndjson_with_large_batch() {
        let result = SearchResult {
            pit_id: "sample_pit_id".to_string(),
            hits: Hits {
                hits: (0..10_000)
                    .map(|i| Hit {
                        _source: json!({ "field": format!("value{}", i) }),
                        sort: vec![i as u64],
                    })
                    .collect(),
            },
        };
        let mut output = Cursor::new(Vec::new());
        persist_ndjson(&result, "test_index", &mut output).await.unwrap();
        let output_str = String::from_utf8(output.into_inner()).unwrap();
        let lines: Vec<&str> = output_str.lines().collect();
        assert_eq!(lines.len(), 20_000); // Each document has an action line
        assert_eq!(lines[0], r#"{"index":{"_index":"test_index"}}"#);
        assert_eq!(lines[1], r#"{"field":"value0"}"#);
        assert_eq!(lines[2], r#"{"index":{"_index":"test_index"}}"#);
        assert_eq!(lines[3], r#"{"field":"value1"}"#);
        assert_eq!(lines[19998], r#"{"index":{"_index":"test_index"}}"#);
        assert_eq!(lines[19999], r#"{"field":"value9999"}"#);
    }

    #[tokio::test]
    async fn test_persist_with_multiple_indices() {
        let search_result1 = create_sample_search_result();
        let search_result2 = SearchResult {
            pit_id: "sample_pit_id_2".to_string(),
            hits: Hits {
                hits: vec![
                    Hit {
                        _source: json!({"field": "value3"}),
                        sort: vec![3],
                    },
                    Hit {
                        _source: json!({"field": "value4"}),
                        sort: vec![4],
                    },
                ],
            },
        };

        let mut output = Cursor::new(Vec::new());
        persist_ndjson(&search_result1, "index1", &mut output).await.unwrap();
        persist_ndjson(&search_result2, "index2", &mut output).await.unwrap();
        let output_str = String::from_utf8(output.into_inner()).unwrap();
        let expected_output = r#"{"index":{"_index":"index1"}}
{"field":"value1"}
{"index":{"_index":"index1"}}
{"field":"value2"}
{"index":{"_index":"index2"}}
{"field":"value3"}
{"index":{"_index":"index2"}}
{"field":"value4"}
"#;
        assert_eq!(output_str, expected_output);
    }
}
