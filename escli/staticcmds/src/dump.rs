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
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

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
}

#[derive(Deserialize, Debug)]
struct PontInTime {
    id: String,
}

#[derive(Deserialize, Debug)]
struct SearchResult {
    pit_id: String,
    hits: Hits,
}

#[derive(Deserialize, Debug)]
struct Hits {
    hits: Vec<Hit>,
}

#[derive(Deserialize, Debug)]
struct Hit {
    _source: Value,
    sort: Vec<u64>,
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
        let mut docs: HashMap<String, Vec<Value>> = HashMap::new();

        for index in indices {
            docs.insert(index.to_string(), Vec::new());
            let buf = docs.get_mut(index).expect("Index should exist in the map");

            let pit_response = client
                .open_point_in_time(OpenPointInTimeParts::Index(&[index]))
                .keep_alive(&self.keep_alive)
                .request_timeout(t)
                .send()
                .await?;
            let initial_pit = pit_response.json::<PontInTime>().await?;

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
            let initial_documents = initial_search.json::<SearchResult>().await?;
            buf.extend(
                initial_documents
                    .hits
                    .hits
                    .iter()
                    .map(|hit| hit._source.clone()),
            );

            let mut next_pit = initial_documents.pit_id;
            let mut next_search_after = initial_documents
                .hits
                .hits
                .last()
                .and_then(|hit| hit.sort.first().cloned());

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
                let pit_json = search_response.text().await?;
                let documents: SearchResult = serde_json::from_str(&pit_json).map_err(|e| {
                    eprintln!("Failed to parse response: {e}\nRaw JSON: {pit_json}");
                    elasticsearch::Error::from(e)
                })?;

                if documents.hits.hits.is_empty() {
                    break;
                }

                buf.extend(documents.hits.hits.iter().map(|hit| hit._source.clone()));
                next_pit = documents.pit_id;
                next_search_after = documents
                    .hits
                    .hits
                    .last()
                    .and_then(|hit| hit.sort.first().cloned());
            }
        }

        // Serialize the documents to ndjson format with elasticsearch header for bulk
        // operations
        // Each document is prefixed with an action line
        // { "index": { "_index": "<index_name>" } }
        let mut ndjson_output = String::new();
        for (index, docs) in docs {
            for doc in docs {
                let action_line = json!({ "index": { "_index": index } });
                ndjson_output.push_str(&action_line.to_string());
                ndjson_output.push('\n');
                ndjson_output.push_str(&doc.to_string());
                ndjson_output.push('\n');
            }
        }

        let hr = http::response::Response::new(ndjson_output);
        let rr = reqwest::Response::from(hr);
        Ok(Response::new(rr, elasticsearch::http::Method::Get))
    }
}
