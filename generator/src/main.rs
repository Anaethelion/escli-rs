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

// Main module for generating CLI commands and namespace files.
//
// This module handles the generation of CLI commands, error handling, and namespace files
// based on the Elasticsearch schema. It includes functionality for downloading the schema,
// parsing it, and generating Rust code for endpoints and namespaces.
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
use tokio::fs::{OpenOptions, read_to_string};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use clap::{CommandFactory, Parser};
use clients_schema::IndexedModel;
use std::collections::HashSet;
use std::io::SeekFrom;
use std::path::Path;

// Represents the CLI options for the generator.
//
// This struct defines the available command-line arguments for the generator,
// including the branch to fetch the schema from.
#[derive(Parser)]
struct Options {
    // Specifies the branch to fetch the schema from. Defaults to "main".
    #[clap(help = "Branch to fetch the schema from, default to main")]
    branch: Option<String>,
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

// Entry point for the generator.
//
// This asynchronous function handles the entire process of generating CLI commands
// and namespace files. It downloads the schema, parses it, filters endpoints, and
// generates the necessary Rust code.
//
// # Returns
//
// A `Result` indicating success or failure.
#[tokio::main]
async fn main() -> Result<(), Error> {
    // Parse CLI options
    let options = Options::command().get_matches();
    let spec: String;

    // Set up output paths
    let binpath = Path::new("escli").join("src");
    let output_dir = "namespaces";

    // Download or read the schema file
    let schema_tmp_path = Path::new("schema.json");
    if !schema_tmp_path.exists() {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(schema_tmp_path)
            .await?;

        // Determine schema URL based on branch option
        let url = match options.get_one::<String>("branch") {
            Some(branch) => format!("https://raw.githubusercontent.com/elastic/elasticsearch-specification/{branch}/output/schema/schema.json"),
            None => "https://raw.githubusercontent.com/elastic/elasticsearch-specification/main/output/schema/schema.json".to_string(),
        };

        // Download and save schema
        spec = reqwest::get(url).await?.text().await?;
        file.write_all(spec.as_bytes()).await?;
    } else {
        // Read schema from local file
        spec = read_to_string(schema_tmp_path).await?;
    }

    // Parse the schema into a model
    let model: &IndexedModel = &serde_json::from_str(&spec)?;

    // Filter and sort endpoints
    let mut endpoints: Vec<endpoint::Endpoint> = model
        .endpoints
        .iter()
        .filter(|e| e.name != "knn_search" && !e.name.starts_with("_internal"))
        .map(|e| endpoint::Endpoint::new(e, model))
        .collect();
    endpoints.sort_by_key(|e| e.e.name.clone());

    // Collect and sort unique namespaces
    let mut namespaces: Vec<String> = endpoints
        .iter()
        .map(|e| e.namespace().clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    namespaces.sort();

    // Ensure output directory exists
    fs::create_dir_all(binpath.clone()).await?;

    // Generate main CLI and error files
    fs::write(
        binpath.join("main.rs"),
        format!("{LICENSE}\n{}", cli::generate().to_string()?),
    )
    .await?;
    fs::write(
        binpath.join("cmd.rs"),
        format!(
            "{LICENSE}\n{}",
            cmd::generate(endpoints.clone()).to_string()?
        ),
    )
    .await?;
    fs::write(
        binpath.join("error.rs"),
        format!("{LICENSE}\n{}", esclierror::generate().to_string()?),
    )
    .await?;

    // Generate mod.rs for namespaces
    let endpoints_path = binpath.join(output_dir).join("mod.rs");
    fs::create_dir_all(endpoints_path.parent().unwrap()).await?;
    fs::write(
        endpoints_path,
        format!("{LICENSE}\n{}", module::generate(&namespaces).to_string()?),
    )
    .await?;

    // Remove old namespace files
    for namespace in &namespaces {
        let namespace_path = binpath
            .join(output_dir)
            .join(namespace.replace(".", "_") + ".rs");
        fs::remove_file(&namespace_path).await.ok();
    }

    // Create enums.rs and write header
    let mut enums_file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(binpath.join("enums.rs"))
        .await?;
    enums_file
        .write_all(format!("{LICENSE}\n").as_bytes())
        .await?;
    enums_file.write_all(b"use serde::Serialize;\n").await?;

    // Generate code for endpoints and enums
    let mut namespace_with_enums = HashSet::new();
    let mut rendered_enums = HashSet::new();

    for endpoint in &endpoints {
        let file_path = binpath.join(output_dir).join(endpoint.namespace() + ".rs");
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(file_path)
            .await?;
        file.write_all(endpoint.clone().generate().to_string()?.as_ref())
            .await?;
        file.write_all(b"\n\n").await?;

        for (name, enum_) in endpoint.enums() {
            if rendered_enums.insert(name.name.to_string()) {
                enums_file
                    .write_all(enum_.clone().generate().to_string()?.as_ref())
                    .await?;
                enums_file.write_all(b"\n\n").await?;
            }
            namespace_with_enums.insert(endpoint.namespace().to_string());
        }
    }

    // Write headers for namespace files
    for namespace in &namespaces {
        let namespace_path = binpath.join(output_dir).join(format!("{namespace}.rs"));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&namespace_path)
            .await?;
        let mut buf = String::new();
        file.read_to_string(&mut buf).await?;
        file.seek(SeekFrom::Start(0)).await?;

        let header = namespace::NamespaceFileHeader {
            with_enums: namespace_with_enums.contains(namespace),
            with_input: endpoints
                .iter()
                .any(|e| e.namespace() == *namespace && e.has_request()),
        };
        file.write_all(format!("{LICENSE}\n").as_bytes()).await?;
        header.write_to(&mut file).await?;
        file.write_all(buf.as_ref()).await?;
    }

    Ok(())
}
