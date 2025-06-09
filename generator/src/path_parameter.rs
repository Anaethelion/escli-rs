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

use genco::prelude::quoted;
use genco::{Tokens, quote};
use std::collections::{HashMap, HashSet};

// Represents metadata for a path parameter.
pub struct PathParameter {
    // The URL path for the endpoint.
    path: String,
    // A list of endpoint parameters.
    endpoints_params: Vec<String>,
    // A set of mandatory parameters for the path.
    mandatory_parameters: HashSet<String>,
    // A set of optional parameters for the path.
    optional_parameters: HashSet<String>,
    // The HTTP method for the path.
    method: String,
}

impl PathParameter {
    // Creates a new `PathParameter` instance.
    //
    // This constructor initializes a `PathParameter` object with the provided
    // path, endpoint parameters, mandatory parameters, optional parameters, and HTTP method.
    //
    // # Arguments
    //
    // * `path` - A `String` representing the URL path for the endpoint.
    // * `endpoints_params` - A `Vec<String>` containing the names of endpoint parameters.
    // * `mandatory_parameters` - A `HashSet<String>` containing the mandatory parameters for the path.
    // * `optional_parameters` - A `HashSet<String>` containing the optional parameters for the path.
    // * `method` - A `String` representing the HTTP method for the path.
    //
    // # Returns
    //
    // A new instance of `PathParameter`.
    pub fn new(
        path: String,
        endpoints_params: Vec<String>,
        mandatory_parameters: HashSet<String>,
        optional_parameters: HashSet<String>,
        method: String,
    ) -> Self {
        Self {
            path,
            endpoints_params,
            mandatory_parameters,
            optional_parameters,
            method,
        }
    }

    // Generates the token representation for the path parameter.
    //
    // This function creates the logic for matching the path and method based on
    // the presence of parameters.
    //
    // # Returns
    //
    // A `Tokens` object representing the match logic for the path parameter.
    pub fn generate(&self) -> Tokens {
        if self.params().is_empty() {
            quote! {
                _ => {
                    (
                    $(quoted(&self.path)).into(),
                    Method::$(self.method.clone())
                    )
                }$['\r']
            }
        } else {
            quote! {
                $(self.pattern_params()) => {
                    (
                    format!($(quoted(&self.path))),
                    Method::$(self.method.clone())
                    )
                }$['\r']
            }
        }
    }

    // Retrieves all parameters (mandatory and optional) for the path.
    //
    // # Returns
    //
    // A `Vec<String>` containing the names of all parameters.
    pub fn params(&self) -> Vec<String> {
        self.optional_parameters
            .iter()
            .chain(self.mandatory_parameters.iter())
            .cloned()
            .collect()
    }

    // Generates the pattern for matching parameters in the path.
    //
    // This function constructs the match pattern for the parameters, ensuring
    // that optional parameters are wrapped in `Some` and mandatory parameters
    // are included directly.
    //
    // # Returns
    //
    // A `String` representing the match pattern for the parameters.
    pub fn pattern_params(&self) -> String {
        let mut params: Vec<String> = vec![];
        let mut code_params = HashMap::new();
        for param in self.mandatory_parameters.iter() {
            code_params.insert(param.clone(), param.clone());
        }
        for param in self.optional_parameters.iter() {
            code_params.insert(param.clone(), format!("Some({param})"));
        }

        for param in &self.endpoints_params {
            if code_params.contains_key(param) {
                if let Some(p) = code_params.get(param) {
                    params.push(p.clone());
                }
            } else {
                params.push("None".to_string());
            }
        }

        match params.len() {
            1 => params.join(""),
            _ => {
                format!("({})", params.join(","))
            }
        }
    }

    pub fn method(&self) -> String {
        self.method.clone()
    }

    pub fn path(&self) -> String {
        self.path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_returns_combined_optional_and_mandatory_parameters() {
        let path_param = PathParameter {
            path: "example_path".to_string(),
            endpoints_params: vec![],
            mandatory_parameters: HashSet::from(["param1".to_string(), "param2".to_string()]),
            optional_parameters: HashSet::from(["param3".to_string()]),
            method: "GET".to_string(),
        };
        let mut result = path_param.params();
        result.sort();
        assert_eq!(
            result,
            vec![
                "param1".to_string(),
                "param2".to_string(),
                "param3".to_string()
            ]
        );
    }

    #[test]
    fn params_returns_empty_vector_when_no_parameters_exist() {
        let path_param = PathParameter {
            path: "example_path".to_string(),
            endpoints_params: vec![],
            mandatory_parameters: HashSet::new(),
            optional_parameters: HashSet::new(),
            method: "GET".to_string(),
        };
        let result = path_param.params();
        assert!(result.is_empty());
    }

    #[test]
    fn pattern_params_returns_single_mandatory_parameter() {
        let path_param = PathParameter {
            path: "example_path".to_string(),
            endpoints_params: vec!["param1".to_string()],
            mandatory_parameters: HashSet::from(["param1".to_string()]),
            optional_parameters: HashSet::new(),
            method: "GET".to_string(),
        };
        let result = path_param.pattern_params();
        assert_eq!(result, "param1");
    }

    #[test]
    fn pattern_params_returns_single_optional_parameter_wrapped_in_some() {
        let path_param = PathParameter {
            path: "example_path".to_string(),
            endpoints_params: vec!["param1".to_string()],
            mandatory_parameters: HashSet::new(),
            optional_parameters: HashSet::from(["param1".to_string()]),
            method: "GET".to_string(),
        };
        let result = path_param.pattern_params();
        assert_eq!(result, "Some(param1)");
    }

    #[test]
    fn pattern_params_returns_none_for_missing_parameters() {
        let path_param = PathParameter {
            path: "example_path".to_string(),
            endpoints_params: vec!["param1".to_string(), "param2".to_string()],
            mandatory_parameters: HashSet::new(),
            optional_parameters: HashSet::new(),
            method: "GET".to_string(),
        };
        let result = path_param.pattern_params();
        assert_eq!(result, "(None,None)");
    }

    #[test]
    fn pattern_params_returns_combined_mandatory_and_optional_parameters() {
        let path_param = PathParameter {
            path: "example_path".to_string(),
            endpoints_params: vec!["param1".to_string(), "param2".to_string()],
            mandatory_parameters: HashSet::from(["param1".to_string()]),
            optional_parameters: HashSet::from(["param2".to_string()]),
            method: "GET".to_string(),
        };
        let result = path_param.pattern_params();
        assert_eq!(result, "(param1,Some(param2))");
    }

    #[test]
    fn pattern_params_handles_empty_endpoints_params() {
        let path_param = PathParameter {
            path: "example_path".to_string(),
            endpoints_params: vec![],
            mandatory_parameters: HashSet::from(["param1".to_string()]),
            optional_parameters: HashSet::from(["param2".to_string()]),
            method: "GET".to_string(),
        };
        let result = path_param.pattern_params();
        assert_eq!(result, "()");
    }
}
