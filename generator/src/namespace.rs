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

pub struct NamespaceFileHeader {
    pub with_enums: bool,
    pub with_input: bool,
}

impl NamespaceFileHeader {
    pub fn to_header_string(&self) -> String {
        let mut out = String::from(
            "use clap::{Command, CommandFactory, Parser};\n\
             use elasticsearch::http::Method;\n\
             use elasticsearch::http::headers::HeaderMap;\n",
        );

        if self.with_input {
            out.push_str(
                "\nuse tokio::fs::File;\n\
                 use tokio::io;\n\
                 use tokio::io::{BufReader, AsyncReadExt};\n\
                 use std::io::IsTerminal;\n\n",
            );
        }

        if self.with_enums {
            out.push_str("use crate::enums::*;");
        }

        out.push_str(
            "\nuse crate::error;\n\
             use crate::namespaces::TransportArgs;\n\
             use crate::namespaces::parse_header;\n\
             use crate::namespaces::Executor;\n\n",
        );

        out
    }
}
