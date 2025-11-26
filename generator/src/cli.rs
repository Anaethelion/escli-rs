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

        use tokio::io;
        use tokio::io::AsyncWriteExt;
        use clap::error::ErrorKind;
        use clap::{FromArgMatches as _, Parser, ArgAction};
        use dotenv::dotenv;
        use elasticsearch::cert::CertificateValidation;
        use elasticsearch::http::Url;
        use elasticsearch::http::transport::{SingleNodeConnectionPool, TransportBuilder};

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

            #[clap(action=ArgAction::SetTrue, default_value_t=false, short, long, env = "ESCLI_VERBOSE", help = "Enable verbose output", long_help = "Enable verbose output for debugging purposes. This will print additional information about the requests and responses.")]
            verbose: bool,
        }

        // Entry point for the CLI application.
        //
        // This asynchronous function initializes the CLI application, parses command-line arguments,
        // and executes the appropriate subcommand logic.
        //
        // # Returns
        //
        // A `Result` indicating success or failure.
        #[tokio::main]
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

            let mut stdout = io::stdout();
            let mut stderr = io::stderr();

            let res: Result<elasticsearch::http::response::Response, elasticsearch::Error>;
            // Check if the subcommand is "utils" to run static commands
            if matches.subcommand_matches("utils").is_some() {
                res = staticcmds::run_command(cmd, matches.subcommand().unwrap().1, transport, config.timeout).await;
            } else {
                let args = cmd::dispatch(&mut cmd, &matches).await?;
                if config.verbose {
                    let qs = serde_urlencoded::to_string(&args.query_string).unwrap_or_default();
                    stderr.write(format!("Request: {:?} {}?{}\n", args.method, args.path, qs).as_bytes()).await.ok();

                    if !&args.headers.is_empty() {
                        stderr.write("Headers:\n".as_bytes()).await.ok();
                        for (k, v) in &args.headers {
                            stderr.write(format!("{}: {:?}\n", k, v).as_bytes()).await.ok();
                        }
                    }
                    stderr.write("\n".as_bytes()).await.ok();
                    stderr.flush().await.ok();
                }
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
                    let istatus_code = res.status_code().as_u16() as i32;
                    let headers = res.headers().clone();
                    let body = res.text().await?;

                    if config.verbose {
                        stderr.write_all(format!("Response: {}\n", istatus_code).as_bytes()).await.ok();
                        if !headers.is_empty() {
                            stderr.write_all("Headers:\n".as_bytes()).await.ok();
                            for (k, v) in headers {
                                if let Some(k) = k {
                                    stderr.write_all(format!("{}: {:?}\n", k, v).as_bytes()).await.ok();
                                }
                            }
                        }
                        stderr.write_all("\n".as_bytes()).await.ok();
                        stderr.flush().await.ok();
                    }

                    // Is status code 2xx or 3xx, write the body to stdout
                    // Otherwise, write the body to stderr
                    if (200..400).contains(&istatus_code) {
                        match stdout.write_all(body.as_bytes()).await {
                            Err(e) if e.kind() != io::ErrorKind::BrokenPipe => {
                                tokio::io::stderr()
                                    .write_all(format!("Error writing to stdout: {e}").as_bytes())
                                    .await.ok();
                                Ok(())
                            }
                            _ => {
                                stdout.flush().await.ok();
                                Ok(())
                            }
                        }
                    } else {
                        if let Err(e) = stderr.write_all(body.as_bytes()).await {
                            if e.kind() != io::ErrorKind::BrokenPipe {
                                tokio::io::stderr()
                                    .write_all(format!("Error writing to stderr: {e}").as_bytes())
                                    .await
                                    .ok();
                            }
                        }
                        std::process::exit(1);
                    }
                }
                Err(err) => {
                    if let Err(e) = tokio::io::stderr()
                        .write_all(format!("{}", error::EscliError::from(err)).as_bytes())
                        .await
                    {
                        if e.kind() != std::io::ErrorKind::BrokenPipe {}
                    }
                    std::process::exit(1);
                }
            }
        }
    }
}
