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

mod dump;

pub use crate::dump::Dump;
use clap::error::ErrorKind;
use clap::{ArgMatches, Command, FromArgMatches};
use elasticsearch::http::response::Response;
use elasticsearch::http::transport::Transport;

pub fn commands() -> [Command; 1] {
    [Dump::new_command()]
}

pub async fn run_command(
    mut cmd: Command,
    matches: &ArgMatches,
    transport: Transport,
    timeout: Option<std::time::Duration>,
) -> Result<Response, elasticsearch::Error> {
    match matches.subcommand() {
        Some(("dump", sub_matches)) => {
            Dump::from_arg_matches(sub_matches)
                .expect("argument parsing failed")
                .execute(transport, timeout)
                .await
        }
        _ => {
            if let Some(namespace_command) = cmd.find_subcommand_mut("utils") {
                let _ = namespace_command.print_help();
            }
            println!();
            cmd.error(ErrorKind::InvalidSubcommand, "unrecognized subcommand")
                .exit();
        }
    }
}
