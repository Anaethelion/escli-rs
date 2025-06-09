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

use convert_case::{Case, Casing};
use genco::prelude::*;

// Represents an enumeration with a name and a list of members.
//
// This struct is used to define and generate Rust enums with associated
// functionality such as serialization, deserialization, and string conversion.
#[derive(Debug, Clone)]
pub(crate) struct Enum {
    // The name of the enumeration.
    name: String,
    // The list of members in the enumeration.
    members: Vec<String>,
}

impl Enum {
    // Creates a new `Enum` instance.
    //
    // # Arguments
    //
    // * `name` - The name of the enumeration.
    // * `members` - A vector of strings representing the members of the enumeration.
    //
    // # Returns
    //
    // A new `Enum` instance.
    pub fn new(name: &str, members: Vec<String>) -> Self {
        Enum {
            name: name.to_string(),
            members,
        }
    }

    // Generates the Rust code for the enumeration.
    //
    // This function creates the Rust code for the enum, including implementations
    // for `std::fmt::Display` and `std::str::FromStr` traits. The generated enum
    // supports serialization and deserialization.
    //
    // # Returns
    //
    // A `Tokens` object containing the generated Rust code.
    pub fn generate(self) -> Tokens {
        quote! {
            // The enumeration definition.
            #[derive(Debug, Copy, Clone, Serialize)]
            pub enum $(&self.name) {
                $(for member in &self.members =>
                    #[serde(rename = $(quoted(member)) )]
                    $(member.to_case(Case::Pascal)),$['\r']
                )
            }

            // Implements the `std::fmt::Display` trait for the enumeration.
            impl std::fmt::Display for $(&self.name) {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    let s = match self {
                        $(
                            for member in &self.members =>
                            Self::$(member.to_case(Case::Pascal)) => $(quoted(member.to_case(Case::Lower))),$['\r']
                        )
                    };
                    write!(f, "{s}")
                }
            }

            // Implements the `std::str::FromStr` trait for the enumeration.
            //
            // This allows parsing a string into an enum variant.
            impl std::str::FromStr for $(&self.name) {
                type Err = String;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    match s {
                        $(
                            for member in &self.members =>
                            $(quoted(member)) => Ok(Self::$(member.to_case(Case::Pascal))),$['\r']
                        )
                        _ => Err(format!("Invalid value for enum {}: {}", stringify!($(&self.name)), s)),
                    }
                }
            }
        }
    }
}
