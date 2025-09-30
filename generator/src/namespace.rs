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

// Represents the header configuration for a namespace file.
//
// This struct contains flags that determine whether certain sections (enums and input handling)
// should be included in the generated namespace file.
pub struct NamespaceFileHeader {
    // Indicates whether enums should be included in the namespace file.
    pub with_enums: bool,
    // Indicates whether input handling should be included in the namespace file.
    pub with_input: bool,
}

impl NamespaceFileHeader {
    // Writes the namespace file header to the provided writer.
    //
    // This function generates the necessary imports and writes them to the given writer.
    // It conditionally includes imports for enums and input handling based on the struct's flags.
    //
    // # Arguments
    //
    // * `writer` - A mutable reference to an object implementing `async_std::io::WriteExt` and `Unpin`.
    //
    // # Returns
    //
    // An `async_std::io::Result<()>` indicating success or failure.
    pub async fn write_to<W: tokio::io::AsyncWriteExt + Unpin>(
        &self,
        mut writer: W,
    ) -> tokio::io::Result<()> {
        // Write common imports to the writer.
        writer
            .write_all(
                b"use clap::{Command, CommandFactory, Parser};
use elasticsearch::http::Method;
use elasticsearch::http::headers::HeaderMap;
",
            )
            .await?;

        // Conditionally include input handling imports if `with_input` is true.
        if self.with_input {
            writer
                .write_all(
                    b"
use async_std::fs::File;
use async_std::io;
use async_std::io::{BufReader, ReadExt};
use atty::Stream;

",
                )
                .await?;
        }

        // Conditionally include enums imports if `with_enums` is true.
        if self.with_enums {
            writer.write_all(b"use crate::enums::*;").await?;
        }

        // Use the shared parse_header from namespaces mod
        writer
            .write_all(
                b"
use crate::error;
use crate::namespaces::TransportArgs;
use crate::namespaces::parse_header;
use crate::namespaces::Executor;",
            )
            .await?;

        // Write a newline to separate sections.
        writer.write_all(b"\n\n").await?;

        Ok(())
    }
}
