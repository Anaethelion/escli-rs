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

mod cli;
mod cmd;
mod endpoint;
mod enumeration;
mod esclierror;
mod field;
mod module;
mod namespace;
mod path_parameter;

use anyhow::Error;
use tokio::fs;
use tokio::fs::read_to_string;
use clap::{CommandFactory, Parser};
use clients_schema::IndexedModel;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const EXCLUDED_ENDPOINTS: &[&str] = &["knn_search"];
const EXCLUDED_PREFIXES: &[&str] = &["_internal"];

#[derive(Parser)]
struct Options {
    #[clap(help = "Branch to fetch the schema from, default to main")]
    branch: Option<String>,
}

fn schema_cache_path(branch: &str) -> PathBuf {
    PathBuf::from(format!("schema-{branch}.json"))
}

static LICENSE: &str = r#"// Licensed to Elasticsearch B.V. under one or more contributor
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
"#;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let options = Options::command().get_matches();
    let branch = options
        .get_one::<String>("branch")
        .map_or("main", |s| s.as_str());

    let binpath = Path::new("escli").join("src");
    let output_dir = "namespaces";

    // Branch-aware schema caching with atomic download
    let cache_path = schema_cache_path(branch);
    let spec = if cache_path.exists() {
        read_to_string(&cache_path).await?
    } else {
        let url = format!(
            "https://raw.githubusercontent.com/elastic/elasticsearch-specification/{branch}/output/schema/schema.json"
        );
        let body = reqwest::get(&url).await?.text().await?;
        let tmp_path = cache_path.with_extension("json.tmp");
        fs::write(&tmp_path, &body).await?;
        fs::rename(&tmp_path, &cache_path).await?;
        body
    };

    let model: &IndexedModel = &serde_json::from_str(&spec)?;

    let mut endpoints: Vec<endpoint::Endpoint> = model
        .endpoints
        .iter()
        .filter(|e| {
            !EXCLUDED_ENDPOINTS.contains(&e.name.as_str())
                && !EXCLUDED_PREFIXES.iter().any(|p| e.name.starts_with(p))
        })
        .map(|e| endpoint::Endpoint::new(e, model))
        .collect();
    endpoints.sort_by(|a, b| a.e.name.cmp(&b.e.name));

    let mut namespaces: Vec<String> = endpoints
        .iter()
        .map(|e| e.namespace())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    namespaces.sort();

    fs::create_dir_all(binpath.clone()).await?;

    fs::write(
        binpath.join("main.rs"),
        format!("{LICENSE}\n{}", cli::generate().to_string()?),
    )
    .await?;
    fs::write(
        binpath.join("cmd.rs"),
        format!(
            "{LICENSE}\n{}",
            cmd::generate(&endpoints).to_string()?
        ),
    )
    .await?;
    fs::write(
        binpath.join("error.rs"),
        format!("{LICENSE}\n{}", esclierror::generate().to_string()?),
    )
    .await?;

    let ns_dir = binpath.join(output_dir);
    fs::create_dir_all(&ns_dir).await?;
    fs::write(
        ns_dir.join("mod.rs"),
        format!("{LICENSE}\n{}", module::generate(&namespaces).to_string()?),
    )
    .await?;

    // Accumulate all namespace content and enum content in memory
    let mut namespace_content: HashMap<String, String> = HashMap::new();
    let mut enums_content = format!("{LICENSE}\nuse serde::Serialize;\n");
    let mut namespace_with_enums: HashSet<String> = HashSet::new();
    let mut rendered_enums: HashSet<String> = HashSet::new();

    for endpoint in &endpoints {
        let ns = endpoint.namespace();
        let code = endpoint.generate().to_string()?;
        namespace_content
            .entry(ns.clone())
            .or_default()
            .push_str(&format!("{code}\n\n"));

        let mut sorted_enums: Vec<_> = endpoint.enums().iter().collect();
        sorted_enums.sort_by_key(|(name, _)| name.name.clone());
        for (name, enum_) in sorted_enums {
            if rendered_enums.insert(name.name.to_string()) {
                enums_content.push_str(&enum_.generate().to_string()?);
                enums_content.push_str("\n\n");
            }
            namespace_with_enums.insert(ns.clone());
        }
    }

    fs::write(binpath.join("enums.rs"), &enums_content).await?;

    // Write each namespace file with header prepended
    for namespace in &namespaces {
        let header = namespace::NamespaceFileHeader {
            with_enums: namespace_with_enums.contains(namespace),
            with_input: endpoints
                .iter()
                .any(|e| e.namespace() == *namespace && e.has_request()),
        };
        let body = namespace_content.get(namespace).map_or("", |s| s.as_str());
        let full_content = format!("{LICENSE}\n{}{body}", header.to_header_string());

        let file_path = ns_dir.join(format!("{namespace}.rs"));
        fs::write(&file_path, &full_content).await?;
    }

    // Format all generated files
    let status = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2024")
        .args(
            std::fs::read_dir(&binpath)?
                .chain(std::fs::read_dir(&ns_dir)?)
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "rs")),
        )
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => eprintln!("rustfmt exited with status: {s}"),
        Err(e) => eprintln!("Failed to run rustfmt (is it installed?): {e}"),
    }

    Ok(())
}
