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

use genco::prelude::quoted;
use genco::{Tokens, quote};
use std::collections::HashMap;

use crate::endpoint;

pub(crate) fn generate(endpoints: Vec<endpoint::Endpoint>) -> Tokens {
    let core_endpoints = endpoints
        .iter()
        .filter(|e| e.namespace() == "core")
        .cloned()
        .collect::<Vec<endpoint::Endpoint>>();

    let endpoints_by_namespace: HashMap<String, Vec<endpoint::Endpoint>> =
        endpoints.iter().fold(HashMap::new(), |mut acc, e| {
            let endpoints = acc.entry(e.namespace().clone()).or_default();
            endpoints.push(e.clone());
            acc
        });

    quote! {
        use std::collections::HashMap;
        use crate::{Config, namespaces, error};
        use clap::{ArgMatches, Command, CommandFactory, FromArgMatches};
        use clap::error::ErrorKind;

        // Generates the main command for the CLI application.
        type Registry = HashMap<String, Box<dyn Fn(&ArgMatches) -> Box<dyn namespaces::Executor>>>;

        pub async fn dispatch(cmd: &mut Command, matches: &ArgMatches) -> Result<namespaces::TransportArgs, error::EscliError> {
            let mut registry: Registry = HashMap::new();

            $(for (_, endpoints) in &endpoints_by_namespace =>
                $(for endpoint in endpoints =>
                    $(&endpoint.clone().generate_executor())$['\r']
                )
            )

            if let Some((namespace, sub_matches)) = matches.subcommand() {
                if let Some((command, arg_matches)) = sub_matches.subcommand() {
                    if let Some(executor) = registry.get(&format!("{namespace}:{command}")) {
                        let args = executor(arg_matches).execute().await;
                        match args {
                            Ok(transport_args) => {
                                return Ok(transport_args);
                            }
                            Err(e) => {
                                eprintln!("Error executing command '{}': {}", command, e);
                                return Err(e);
                            }
                        }
                    } else {
                        if let Some(namespace_command) = cmd.find_subcommand_mut(namespace) {
                            let _ = namespace_command.print_help();
                        }
                        println!();
                        cmd.error(ErrorKind::InvalidSubcommand, "unrecognized subcommand")
                            .exit();
                    }
                } else if let Some((command, arg_matches)) = matches.subcommand() {
                    if let Some(executor) = registry.get(&format!("core:{command}")) {
                        let args = executor(arg_matches).execute().await;
                        match args {
                            Ok(transport_args) => {
                                return Ok(transport_args);
                            }
                            Err(e) => {
                                eprintln!("Error executing command '{command}': {e}");
                                return Err(e);
                            }
                        }
                    } else {
                        if let Some(namespace_command) = cmd.find_subcommand_mut(command) {
                            let _ = namespace_command.print_help();
                        }
                        println!();
                        cmd.error(ErrorKind::InvalidSubcommand, "unrecognized subcommand")
                            .exit();
                    }
                }
            }

            Err(error::EscliError::new("No subcommand provided or command not found"))
        }

        // Generates the main CLI command.
        //
        // This function defines the structure of the CLI application, including subcommands
        // for namespaces and endpoints.
        //
        // # Returns
        //
        // A `Command` object representing the CLI application.
        pub fn command() -> Command {
            let after_help_heading: &str = color_print::cstr!(r#"<underline><bold>Examples:</bold><underline>"#);
            let after_help: String = format!(
        "{}{}",
        after_help_heading,
        $("r#\"
./escli info
./escli bulk --input <file.ndjson>
./escli search <<< '{\"query\": {\"match_all\": {}}}'
./escli esql query --format txt <<< 'FROM <index> LIMIT 10'
\"#")
        );
            Config::command()
                .name("escli")
                .author("Elastic")
                .version(env!("CARGO_PKG_VERSION"))
                .about("You know, for search.")
                .long_about("The shortest way between your cli and your cluster. You know, for search.")
                .subcommand_required(true)
                .after_help(after_help)
                .subcommand(
                    Command::new("utils")
                        .about("Utility commands")
                        .subcommands(staticcmds::commands())
                )
                .subcommands([
                    $(for endpoint in &core_endpoints =>
                        $(endpoint.clone().generate_new_command())
                    )
                ])
                $(for (namespace, endpoints) in &endpoints_by_namespace =>
                    .subcommand(
                        Command::new($(quoted(namespace)))
                        .subcommands([
                            $(for endpoint in endpoints =>
                                $(endpoint.clone().generate_new_command())
                            )
                        ])
                    )$['\r']
                )
        }
    }
}
