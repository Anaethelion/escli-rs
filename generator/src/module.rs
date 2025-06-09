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

pub fn generate(namespaces: &[String]) -> Tokens {
    quote! {
        use elasticsearch::http::headers::HeaderMap;
        use elasticsearch::http::Method;

        use crate::error;

        $(for namespace in namespaces =>
            pub mod $(namespace.replace(".", "_"));$['\r']
        )

        // Shared header parser for all namespaces
        pub fn parse_header(s: &str) -> Result<(String, String), String> {
            let (k, v) = s.split_once(":")
                .ok_or_else(|| "Header must be in 'Key:Value' format".to_string())?;
            let k = k.trim();
            let v = v.trim();
            if k.is_empty() || v.is_empty() {
                return Err("Header key and value cannot be empty".to_string());
            }
            Ok((k.to_string(), v.to_string()))
        }

        pub struct TransportArgs {
            pub method: Method,
            pub path: String,
            pub headers: HeaderMap,
            pub query_string: Box<dyn erased_serde::Serialize>,
            pub body: Option<String>,
        }

        #[async_trait::async_trait]
        pub trait Executor {
            async fn execute(&self) -> Result<TransportArgs, error::EscliError>;
        }
    }
}
