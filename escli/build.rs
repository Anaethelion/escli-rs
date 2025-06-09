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

// Temporary fix for MSVC stack overflow issue
fn main() {
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or("".to_owned());
    if target_env == "msvc" {
        println!("cargo:rustc-link-arg=/stack:{}", 3 * 1024 * 1024);
    }
}
