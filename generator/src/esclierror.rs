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

// Generates the error handling code for the CLI application.
//
// This function defines the `EscliError` enum and its implementations for various error types.
//
// # Returns
//
// A `Tokens` object containing the generated error handling code.
pub(crate) fn generate() -> Tokens {
    quote! {
        use std::convert::Infallible;
        use std::error::Error;
        use std::fmt::{Display, Formatter};
        use clap::error::ErrorKind;

        #[doc=" Represents errors that can occur in the CLI application."]
        #[derive(Debug)]
        pub enum EscliError {
            #[doc=" Indicates a configuration error."]
            Config(String),
            #[doc=" Indicates a transport error."]
            Transport(String),
            #[doc=" Indicates a command error."]
            Command(String),
            #[doc=" Indicates an execution error."]
            Execution(String),
            #[doc=" Indicates an I/O error."]
            Io(String)
        }

        impl EscliError {
            pub(crate) fn new(error: &str) -> EscliError {
                EscliError::Command(error.to_string())
            }
        }

        #[doc=" Implements the `Display` trait for `EscliError`."]
        impl Display for EscliError {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                match self {
                    EscliError::Config(msg) => write!(f, "{msg}"),
                    EscliError::Transport(msg) => write!(f, "{msg}"),
                    EscliError::Command(msg) => write!(f, "{msg}"),
                    EscliError::Execution(msg) => write!(f, "{msg}"),
                    EscliError::Io(msg) => write!(f, "{msg}"),
                }
            }
        }

        #[doc=" Converts `BuildError` into `EscliError`."]
        impl From<elasticsearch::http::transport::BuildError> for EscliError {
            fn from(err: elasticsearch::http::transport::BuildError) -> Self {
                EscliError::Transport(format!("Transport error: {err}"))
            }
        }

        #[doc=" Converts `clap::error::Error` into `EscliError`."]
        impl From<clap::error::Error> for EscliError {
            fn from(value: clap::error::Error) -> Self {
                EscliError::Command(format!("Command error: {value}"))
            }
        }

        #[doc=" Converts `Infallible` into `EscliError`."]
        impl From<Infallible> for EscliError {
            fn from(value: Infallible) -> Self {
                EscliError::Command(format!("Infallible error: {value}"))
            }
        }

        #[doc=" Converts `ErrorKind` into `EscliError`."]
        impl From<ErrorKind> for EscliError {
            fn from(value: ErrorKind) -> Self {
                EscliError::Config(format!("Error parsing config file: {value}"))
            }
        }

        #[doc=" Converts `serde_json::error::Error` into `EscliError`."]
        impl From<serde_json::error::Error> for EscliError {
            fn from(value: serde_json::error::Error) -> Self {
                EscliError::Config(format!("Error parsing config file: {value}"))
            }
        }

        impl From<std::io::Error> for EscliError {
            fn from(value: std::io::Error) -> Self {
                EscliError::Io(format!("I/O error: {value}"))
            }
        }

        #[doc = " Converts `elasticsearch::Error` into `EscliError`."]
        impl From<elasticsearch::Error> for EscliError {
            fn from(value: elasticsearch::Error) -> Self {
                if let Some(source) = value.source() {
                    if let Some(reqwest_error) = source.downcast_ref::<reqwest::Error>()
                    {
                        let mut s = format!("Error executing query: {reqwest_error}, ");
                        if let Some(source) = reqwest_error.source() {
                            s.push_str(format!("caused by: {source}").as_str());
                        }
                        return EscliError::Execution(s);
                    }
                }

                EscliError::Execution(format!("Error executing query: {value}"))
            }
        }
    }
}
