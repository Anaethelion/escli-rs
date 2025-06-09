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

use genco::{Tokens, quote};

// Generates the main CLI command structure.
//
// This function organizes endpoints into namespaces and generates the CLI command structure
// for the application. It includes subcommands for each namespace and endpoint.
//
// # Arguments
//
// * `endpoints` - A vector of `Endpoint` objects representing the available endpoints.
//
// # Returns
//
// A `Tokens` object containing the generated CLI command structure.
pub fn generate() -> Tokens {
    quote! {
        mod namespaces;
        mod enums;
        mod error;
        mod cmd;

        use async_std::io;
        use clap::error::ErrorKind;
        use clap::{FromArgMatches as _, Parser};
        use elasticsearch::cert::CertificateValidation;
        use elasticsearch::http::Url;
        use elasticsearch::http::transport::{SingleNodeConnectionPool, TransportBuilder};
        use dotenv::dotenv;
        use async_std::process::exit;
        use async_std::{eprintln};
        use async_std::io::{WriteExt};

        // Represents the configuration options for the CLI application.
        //
        // This struct defines the available command-line arguments and environment variables
        // for configuring the application.
        #[derive(Parser, Debug)]
        #[clap(author, version, about, long_about = None)]
        pub struct Config {
            #[clap(short, long, env = "ESCLI_URL", help = "Elasticsearch cluster url", long_help = "The URL of the Elasticsearch cluster to connect to. This should be in the format 'http://localhost:9200' or 'https://localhost:9200'.")]
            url: Url,

            #[clap(short, long, env = "ESCLI_TIMEOUT", help = "CLI request timeout in seconds", default_value = "60", value_parser = |s: &str| s.parse().map(std::time::Duration::from_secs))]
            timeout: Option<std::time::Duration>,

            #[clap(long, env = "ESCLI_USERNAME", help = "Username for authentication", long_help = "The username for basic authentication with Elasticsearch. This is required if you are not using an API key.")]
            username: Option<String>,

            #[clap(long, env = "ESCLI_PASSWORD", help = "Password for authentication", long_help = "The password for basic authentication with Elasticsearch. This is required if you are not using an API key.")]
            password: Option<String>,

            #[clap(long, env = "ESCLI_API_KEY", help = "API key for authentication encoded as base64.", long_help = "The API key for authentication with Elasticsearch, encoded as base64. This is used for secure access to the Elasticsearch cluster.")]
            api_key: Option<String>,

            #[clap(long, env = "ESCLI_INSECURE", help = "Disable TLS certificate validation (insecure)", long_help = "Disable TLS certificate validation (insecure)")]
            insecure: Option<bool>,
        }

        // Entry point for the CLI application.
        //
        // This asynchronous function initializes the CLI application, parses command-line arguments,
        // and executes the appropriate subcommand logic.
        //
        // # Returns
        //
        // A `Result` indicating success or failure.
        #[async_std::main]
        async fn main() -> Result<(), error::EscliError> {
            clap_complete::CompleteEnv::with_factory(cmd::command).complete();

            dotenv().ok();

            let mut cmd = cmd::command();
            let matches = cmd.clone().get_matches();
            let config = Config::from_arg_matches(&matches)?;

            let transport = if config.insecure.is_some() {
                TransportBuilder::new(SingleNodeConnectionPool::new(config.url))
                    .cert_validation(CertificateValidation::None)
                    .build()?
            } else {
                TransportBuilder::new(SingleNodeConnectionPool::new(config.url)).build()?
            };

            match (&config.api_key, &config.username, &config.password) {
                (Some(_), None, None) => {
                    transport.set_auth(elasticsearch::auth::Credentials::EncodedApiKey(
                        config.api_key.unwrap().clone(),
                    ));
                }

                (None, Some(_), Some(_)) => {
                    transport.set_auth(elasticsearch::auth::Credentials::Basic(
                        config.username.unwrap().clone(),
                        config.password.unwrap().clone(),
                    ));
                }

                (None, Some(_), None) | (None, None, Some(_)) => {
                    cmd.error(
                        ErrorKind::ArgumentConflict,
                        "Both --username and --password must be provided together.",
                    )
                    .exit();
                }

                (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
                    cmd.error(
                        ErrorKind::ArgumentConflict,
                        "Use either --api-key or --username/--password, not both.",
                    )
                    .exit();
                }

                _ => (),
            }

            let res: Result<elasticsearch::http::response::Response, elasticsearch::Error>;
            // Check if the subcommand is "utils" to run static commands
            if matches.subcommand_matches("utils").is_some() {
                res = staticcmds::run_command(cmd, matches.subcommand().unwrap().1, transport, config.timeout).await;
            } else {
                let args = cmd::dispatch(&mut cmd, &matches).await?;
                res = transport.send(
                    args.method,
                    &args.path,
                    args.headers,
                    Some(&args.query_string),
                    args.body,
                    config.timeout,
                ).await;
            }
            match res {
                Ok(res) => {
                    let body = res.text().await?;
                    let mut stdout = io::stdout();
                    if let Err(e) = stdout.write_all(body.as_bytes()).await {
                        if e.kind() != std::io::ErrorKind::BrokenPipe {
                            let _ = async_std::io::stderr().write_all(format!("Error writing to stdout: {}\n", e).as_bytes()).await;
                        }
                    }
                    Ok(())
                }
                Err(err) => {
                    if let Err(e) = async_std::io::stderr().write_all(format!("{}\n", error::EscliError::from(err)).as_bytes()).await {
                        if e.kind() != std::io::ErrorKind::BrokenPipe {
                            // do nothing, we are already exiting
                        }
                    }
                    exit(1);
                }
            }
        }
    }
}
