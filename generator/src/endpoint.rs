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

use crate::enumeration::Enum;
use crate::field::Field;
use crate::path_parameter::PathParameter;

use clients_schema::{Body, IndexedModel, ServerDefault, TypeDefinition, TypeName, ValueOf};
use convert_case::{Case, Casing};
use genco::tokens::quoted;
use genco::{Tokens, quote};
use regex::Regex;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::ops::Sub;

// Represents an API endpoint with its associated metadata and parameters.
//
// This struct encapsulates the details of an API endpoint, including its path
// and query parameters, enums, path selection logic, and whether it requires
// a request body.
#[derive(Debug, Clone)]
pub struct Endpoint {
    // The underlying endpoint definition from the `clients_schema`.
    pub e: clients_schema::Endpoint,
    // A list of fields representing the path parameters for the endpoint.
    path_parameters: Vec<Field>,
    // A list of fields representing the query parameters for the endpoint.
    query_parameters: Vec<Field>,
    // A map of type names to enums used by the endpoint.
    enums: HashMap<clients_schema::TypeName, Enum>,
    // Tokens representing the logic for selecting the appropriate path for the endpoint.
    paths_selection: Tokens,
    // Indicates whether the endpoint requires a request body.
    has_request: bool,
}

impl Endpoint {
    // Creates a new `Endpoint` instance by populating its metadata and parameters.
    //
    // This function initializes an `Endpoint` object with the provided endpoint definition
    // and indexed model. It performs the following tasks:
    // - Populates path parameters.
    // - Populates query parameters.
    // - Generates path selection logic.
    // - Checks if the endpoint requires a request body.
    //
    // # Arguments
    //
    // * `endpoint` - A reference to the `clients_schema::Endpoint` object representing the API endpoint.
    // * `model` - A reference to the `clients_schema::IndexedModel` object containing the schema.
    //
    // # Returns
    //
    // A fully initialized `Endpoint` instance.
    pub fn new(endpoint: &clients_schema::Endpoint, model: &clients_schema::IndexedModel) -> Self {
        let mut e = Endpoint {
            e: endpoint.clone(),
            path_parameters: vec![],
            query_parameters: vec![],
            enums: HashMap::new(),
            paths_selection: Default::default(),
            has_request: false,
        };

        // Populate path parameters based on the schema model.
        e.populate_path_parameters(model);

        // Populate query parameters based on the schema model.
        e.populate_query_parameters(model);

        // Generate the logic for selecting the appropriate path for the endpoint.
        e.generate_path_selection();

        // Check if the endpoint has a request body and update the `has_request` flag accordingly.
        if let Some(r) = e.request(model) {
            match r.body {
                Body::NoBody(_) => {}
                _ => {
                    e.has_request = true;
                }
            }
        }

        e
    }

    // Returns the name of the endpoint, formatted appropriately.
    //
    // This function performs the following tasks:
    // - If the endpoint name contains a dot (`.`), it splits the name and uses the part after the last dot.
    // - If the name is "help", it replaces it with "_help".
    // - Otherwise, it replaces all dots (`.`) in the name with underscores (`_`).
    //
    // # Returns
    //
    // A `String` representing the formatted name of the endpoint.
    fn name(&self) -> String {
        if let Some((_, name)) = self.e.name.rsplit_once('.') {
            if name.eq("help") {
                "_help".to_string()
            } else {
                name.to_string()
            };
        }
        self.e.name.replace(".", "_").to_string()
    }

    // Returns the short name of the endpoint.
    //
    // This function extracts the part of the endpoint name after the last dot (`.`).
    // If the name is "help", it replaces it with "_help". If no dot is found, it
    // falls back to the full name.
    //
    // # Returns
    //
    // A `String` representing the short name of the endpoint.
    fn short_name(&self) -> String {
        if let Some((_, name)) = self.e.name.rsplit_once('.') {
            if name.eq("help") {
                "_help".to_string()
            } else {
                name.to_string()
            }
        } else {
            self.name()
        }
    }

    // Converts the short name of the endpoint to camel case.
    //
    // This function uses the `convert_case` crate to transform the short name
    // into UpperCamelCase format.
    //
    // # Returns
    //
    // A `String` representing the camel case version of the short name.
    fn camel_case_name(&self) -> String {
        self.short_name().to_case(Case::UpperCamel)
    }

    // Returns the namespace of the endpoint.
    //
    // This function extracts the part of the endpoint name before the last dot (`.`).
    // If no dot is found, it defaults to "core".
    //
    // # Returns
    //
    // A `String` representing the namespace of the endpoint.
    pub fn namespace(&self) -> String {
        if let Some((namespace, _)) = self.e.name.rsplit_once('.') {
            namespace.to_string()
        } else {
            "core".to_string()
        }
    }

    // Returns the short description for the endpoint.
    //
    // This function extracts only the first line of the endpoint's description.
    // If the description is empty, it returns an empty string.
    //
    // # Returns
    //
    // A `String` containing the first line of the endpoint's description.
    fn short_description(&self) -> String {
        self.e
            .description
            .clone()
            .split('\n')
            .next()
            .unwrap_or("")
            .to_string()
    }

    // Returns the full description of the endpoint.
    //
    // This function retrieves the complete description of the endpoint and escapes
    // any special characters for safe usage.
    //
    // # Returns
    //
    // A `String` containing the full escaped description of the endpoint.
    fn description(&self) -> String {
        self.e.description.clone().escape_default().to_string()
    }

    // Retrieves the enums associated with the endpoint.
    //
    // This function provides access to the map of type names to enums used by the endpoint.
    //
    // # Returns
    //
    // A reference to a `HashMap` where the keys are `clients_schema::TypeName`
    // and the values are `Enum` objects.
    pub fn enums(&self) -> &HashMap<clients_schema::TypeName, Enum> {
        &self.enums
    }

    // Retrieves the request object for the endpoint.
    //
    // This function attempts to fetch the request object from the indexed model.
    // If the endpoint does not have a request, it returns `None`.
    //
    // # Arguments
    //
    // * `model` - A reference to the `IndexedModel` containing the schema.
    //
    // # Returns
    //
    // An `Option` containing a reference to the `clients_schema::Request` object
    // if it exists, or `None` otherwise.
    fn request<'a>(&self, model: &'a IndexedModel) -> Option<&'a clients_schema::Request> {
        match &self.e.request {
            Some(req) => Some(model.get_request(req).expect("no request")),
            None => None,
        }
    }

    // Populates the query parameters for the endpoint.
    //
    // This function iterates over the query parameters defined in the request object
    // and resolves their types using the schema model. It ensures that query parameters
    // do not overlap with path parameters. Additionally, it processes attached behaviors
    // to include their properties as query parameters.
    //
    // # Arguments
    //
    // * `model` - A reference to the `IndexedModel` containing the schema.
    //
    // # Behavior
    //
    // - Resolves the type of each query parameter using `resolve_value_of`.
    // - Filters out query parameters that overlap with path parameters.
    // - Processes attached behaviors to include their properties as query parameters.
    // - Updates the `query_parameters` field of the `Endpoint` struct.
    pub fn populate_query_parameters(&mut self, model: &IndexedModel) {
        if let Some(req) = self.request(model) {
            let mut query_parameters = req
                .query
                .iter()
                .filter_map(|p| {
                    let ty = self.resolve_value_of(&p.typ, model);
                    let field = Field::new(
                        p.name.clone(),
                        p.description.clone().unwrap_or_default(),
                        p.required,
                        ty,
                        None,
                    );
                    if self
                        .path_parameters
                        .iter()
                        .any(|x| x.name() == field.name())
                    {
                        None
                    } else {
                        Some(field)
                    }
                })
                .collect::<Vec<_>>();

            req.attached_behaviors.iter().for_each(|behavior| {
                let behavior = model
                    .get_interface(&TypeName {
                        namespace: "_spec_utils".into(),
                        name: behavior.into(),
                    })
                    .expect("behavior not found");

                behavior
                    .properties
                    .iter()
                    .filter_map(|p| {
                        let ty = self.resolve_value_of(&p.typ, model);
                        let default_value: Option<String> =
                            p.server_default.as_ref().map(|v| match v {
                                ServerDefault::Boolean(b) => b.to_string(),
                                _ => "".to_string(),
                            });
                        let field = Field::new(
                            p.name.clone(),
                            p.description.clone().unwrap_or_default(),
                            p.required,
                            ty,
                            default_value,
                        );
                        if self
                            .path_parameters
                            .iter()
                            .any(|x| x.name() == field.name())
                        {
                            None
                        } else {
                            Some(field)
                        }
                    })
                    .for_each(|param| {
                        if !query_parameters.iter().any(|x| x.name() == param.name()) {
                            query_parameters.push(param);
                        }
                    });
            });

            self.query_parameters = query_parameters;
        } else {
            self.query_parameters = Vec::new();
        }
    }

    // Populates the path parameters for the endpoint.
    //
    // This function extracts the path parameters from the request object and resolves
    // their types using the schema model. It sorts the parameters by the length of their names
    // in descending order.
    //
    // # Arguments
    //
    // * `model` - A reference to the `IndexedModel` containing the schema.
    //
    // # Behavior
    //
    // - Resolves the type of each path parameter using `resolve_value_of`.
    // - Sorts the path parameters by name length in descending order.
    // - Updates the `path_parameters` field of the `Endpoint` struct.
    pub fn populate_path_parameters(&mut self, model: &IndexedModel) {
        self.path_parameters = if let Some(req) = self.request(model) {
            let mut fields: Vec<_> = req
                .path
                .iter()
                .map(|p| {
                    let ty = self.resolve_value_of(&p.typ, model);
                    Field::new(
                        p.name.clone(),
                        p.description.clone().unwrap_or_default(),
                        p.required,
                        ty,
                        None,
                    )
                })
                .collect();

            fields.sort_by_key(|f| std::cmp::Reverse(f.name().len()));
            fields
        } else {
            Vec::new()
        };
    }

    // Resolves the Rust type for a given `ValueOf` object.
    //
    // This function maps the `ValueOf` object to its corresponding Rust type based on
    // the schema model. It handles built-in types, interfaces, enums, type aliases, and arrays.
    //
    // # Arguments
    //
    // * `v` - A reference to the `ValueOf` object representing the type.
    // * `model` - A reference to the `IndexedModel` containing the schema.
    //
    // # Returns
    //
    // A `String` representing the resolved Rust type.
    //
    // # Behavior
    //
    // - Maps built-in types to their Rust equivalents (e.g., `string` -> `String`).
    // - Resolves interfaces, enums, and type aliases using the schema model.
    // - Handles arrays by returning a placeholder type (`String` for now).
    fn resolve_value_of(&mut self, v: &ValueOf, model: &IndexedModel) -> String {
        match v {
            ValueOf::InstanceOf(i) => {
                if i.typ.namespace == "_builtins" {
                    match i.typ.name.as_str() {
                        "string" => return "String".to_string(),
                        "int" => return "i64".to_string(),
                        "long" => return "i64".to_string(),
                        "float" => return "f32".to_string(),
                        "double" => return "f64".to_string(),
                        "boolean" => return "bool".to_string(),
                        _ => {
                            return "String".to_string();
                        }
                    }
                }
                let td = model.get_type(&i.typ);
                if let Ok(td) = td {
                    match td {
                        TypeDefinition::Interface(i) => i.base.name.to_string(),
                        TypeDefinition::Enum(e) => {
                            self.enums.insert(
                                e.base.name.clone(),
                                Enum::new(
                                    &e.base.name.name,
                                    e.members.iter().map(|m| m.name.clone()).collect(),
                                ),
                            );
                            e.base.name.name.to_string()
                        }
                        TypeDefinition::TypeAlias(t) => self.resolve_value_of(&t.typ, model),
                        _ => "String".to_string(),
                    }
                } else {
                    "String".to_string()
                }
            }
            ValueOf::ArrayOf(_) => {
                // TODO : explore if Vac is really a good idea.
                // format!("Vec<{}>", self.resolve_value_of(a.value.as_ref(), model))
                "String".to_string()
            }
            _ => "String".to_string(),
        }
    }

    // Generates the path selection logic for the endpoint.
    //
    // This function constructs the logic for determining the appropriate URL and HTTP method
    // based on the endpoint's path parameters and optional parameters. It uses a nested structure
    // to handle multiple paths and methods, ensuring that the correct path is selected based on
    // the provided parameters.
    pub fn generate_path_selection(&mut self) {
        let mut toks = Tokens::new();
        let optional_parameters = self.collect_optional_parameters();
        let mut path_params = self.build_path_parameters(&optional_parameters);
        path_params.sort_by_key(|p| Reverse(p.params().len()));
        self.generate_path_selection_tokens(&mut toks, &path_params);
        self.paths_selection = toks.clone();
    }

    /// Collects the set of optional path parameter names.
    fn collect_optional_parameters(&self) -> HashSet<String> {
        self.path_parameters
            .iter()
            .filter_map(|field| {
                if field.required() {
                    None
                } else {
                    Some(field.name().clone())
                }
            })
            .collect()
    }

    /// Builds the list of PathParameter objects for all endpoint URLs.
    fn build_path_parameters(
        &mut self,
        optional_parameters: &HashSet<String>,
    ) -> Vec<PathParameter> {
        let mut path_params: Vec<PathParameter> = vec![];
        let re = Regex::new(r"\{([^}]+)}").expect("regex failed to compile");
        for url in &self.e.urls {
            if (self.e.name == "indices.put_alias" || self.e.name == "indices.delete_alias")
                && url.path.contains("_aliases")
            {
                continue;
            }
            let method = if url.methods.len() == 1 {
                url.methods[0].clone()
            } else if url.methods.contains(&"POST".to_string()) {
                "POST".to_string()
            } else {
                "GET".to_string()
            };
            let params: HashSet<String> = re
                .captures_iter(&url.path)
                .filter_map(|cap| cap.get(1).map(|cap| cap.as_str().to_string()))
                .map(|f| match f.as_str() {
                    "type" => "ty".to_string(),
                    _ => f,
                })
                .collect();
            let endpoints_params: Vec<String> = self
                .path_parameters
                .iter()
                .map(|f| f.name().clone())
                .collect();
            let tmp_params: HashSet<String> = HashSet::from_iter(endpoints_params.clone());
            for param in params.sub(&tmp_params) {
                self.path_parameters.push(Field::new(
                    param.clone(),
                    "".to_string(),
                    true,
                    "String".to_string(),
                    None,
                ));
            }
            path_params.push(PathParameter::new(
                url.path.replace("{type}", "{ty}").clone(),
                endpoints_params,
                params.sub(optional_parameters),
                optional_parameters.intersection(&params).cloned().collect(),
                method.to_case(Case::Pascal),
            ));
        }
        path_params
    }

    /// Generates the path selection tokens for the endpoint.
    fn generate_path_selection_tokens(&self, toks: &mut Tokens, path_params: &[PathParameter]) {
        if path_params.len() == 1 {
            let path_param = path_params.first().unwrap();
            let method = path_param.method();
            let params: Vec<String> = path_param.params().to_vec();
            if path_param.params().is_empty() {
                toks.append(quote! {
                    let url = $(quoted(&path_param.path())).to_string();$['\r']
                });
            } else {
                toks.append(quote!{
                    let url = format!($(quoted(&path_param.path())), $(params.iter().map(|f| format!("{f}=self.{f}")).collect::<Vec<String>>().join(", ")));$['\r']
                });
            }
            toks.append(quote! {
                let method = Method::$(&method);
            });
        } else {
            let parameters_list: Vec<String> = self
                .path_parameters
                .iter()
                .map(|f| format!("&self.{}", f.name().clone()))
                .collect();
            let to_match = match parameters_list.len() {
                1 => parameters_list[0].to_string(),
                _ => format!("({})", parameters_list.join(",")),
            };
            toks.append(quote! {
                let (url, method) = match $(to_match) {
                    $(for path_param in path_params.iter() =>
                        $(&path_param.generate())
                    )
                };
            });
        }
    }

    // Generates the command for creating a new endpoint.
    //
    // This function constructs the logic for generating a new command for the endpoint
    // based on its namespace and camel case name.
    //
    // # Returns
    //
    // A `Tokens` object representing the new command.
    pub fn generate_new_command(self) -> Tokens {
        quote! {
            namespaces::$(&self.namespace())::$(&self.camel_case_name())::new_command(),$['\r']
        }
    }

    pub fn generate_executor(self) -> Tokens {
        quote! {
            //    registry.insert(format!("{namespace}:{}", endpoint.name()), Box::new(|matches| Box::new(namespaces::inference::Update::from_arg_matches(matches).unwrap())));
            registry.insert($(quoted(vec![&self.namespace(), ":", &self.short_name()])).to_string(), Box::new(|matches| Box::new(namespaces::$(&self.namespace())::$(&self.camel_case_name())::from_arg_matches(matches).unwrap())));
        }
    }

    // Retrieves all required fields for the endpoint.
    //
    // This function combines the path parameters and query parameters, filtering
    // only the fields that are marked as required.
    //
    // # Returns
    //
    // A `Vec` containing references to the required `Field` objects.
    fn required_fields(&self) -> Vec<&Field> {
        self.path_parameters
            .iter()
            .chain(self.query_parameters.iter())
            .filter(|f| f.required())
            .collect()
    }

    // Retrieves all optional fields for the endpoint.
    //
    // This function combines the path parameters and query parameters, filtering
    // only the fields that are not marked as required.
    //
    // # Returns
    //
    // A `Vec` containing references to the optional `Field` objects.
    fn optional_fields(&self) -> Vec<&Field> {
        self.path_parameters
            .iter()
            .chain(self.query_parameters.iter())
            .filter(|f| !f.required())
            .collect()
    }

    // Generates the argument definition for the input file.
    //
    // This function creates a CLI argument for specifying an input file or using
    // stdin. The argument is only generated if the endpoint requires a request body.
    //
    // # Returns
    //
    // A `Tokens` object representing the argument definition, or an empty `Tokens`
    // object if the endpoint does not require a request body.
    fn input_arg(&self) -> Tokens {
        match self.has_request {
            true => {
                quote! {
                    #[arg(long, help = "Input file or '-' for stdin")]
                    input: Option<String>,$['\r']
                }
            }
            false => {
                quote! {}
            }
        }
    }

    // Checks whether the endpoint requires a request body.
    //
    // This function determines if the endpoint has a request body based on its
    // metadata.
    //
    // # Returns
    //
    // A `bool` indicating whether the endpoint requires a request body.
    pub fn has_request(&self) -> bool {
        self.has_request
    }

    // Handles input for the endpoint.
    //
    // This function processes the input provided via CLI arguments or stdin. If the endpoint
    // requires a request body, it reads the input from a file, stdin, or checks if stdin is
    // not attached to a terminal.
    //
    // # Behavior
    //
    // - Reads input from a file if a filename is provided.
    // - Reads input from stdin if "-" is specified.
    // - Reads input from stdin if no filename is provided and stdin is not attached to a terminal.
    //
    // # Returns
    //
    // A `Tokens` object representing the input handling logic.
    fn input_handling(&self) -> Tokens {
        match self.has_request {
            true => quote! {
                let mut body = String::new();
                match self.input.as_deref() {
                    Some("-") => {
                        let stdin = io::stdin();
                        let mut reader = BufReader::new(stdin);
                        reader
                            .read_to_string(&mut body).await?;
                    }
                    Some(filename) => {
                        let file = File::open(filename).await?;
                        let mut reader = BufReader::new(file);
                        reader
                            .read_to_string(&mut body).await?;
                    }
                    None => {
                        if !atty::is(Stream::Stdin) {
                            io::stdin().read_to_string(&mut body).await?;
                        }
                    }
                }
            },
            false => quote! {},
        }
    }

    // Generates the CLI command and execution logic for the endpoint.
    //
    // This function defines the CLI command structure, including required and optional fields,
    // and implements the logic for executing the endpoint. It handles query parameters, input
    // handling, and path selection.
    //
    // # Returns
    //
    // A `Tokens` object representing the CLI command and execution logic.
    pub fn generate(self) -> Tokens {
        quote! {
            #[derive(Parser)]
            #[command(name = $(quoted(&self.short_name())))]
            pub struct $(&self.camel_case_name()) {
                $(for field in &self.required_fields() =>
                    $(&field.arg())
                )

                $(for field in &self.optional_fields() =>
                    $(&field.arg())
                )

                $(self.input_arg())

                /// Custom HTTP headers to include in the request. Repeatable.
                #[arg(short = 'H', long = "header", value_name = "HEADER", help = "Add a custom header (key:value)", num_args = 0.., action = clap::ArgAction::Append, value_parser = parse_header)]
                pub header: Vec<(String, String)>,
            }

            impl $(&self.camel_case_name()) {
                // Creates a new CLI command for the endpoint.
                //
                // # Returns
                //
                // A `Command` object representing the CLI command.
                pub fn new_command() -> Command {
                    Self::command()
                    .about($(quoted(&self.short_description())))
                    .long_about($(quoted(self.description())))
                }
            }

            #[async_trait::async_trait]
            impl Executor for $(&self.camel_case_name()) {
                                // Executes the endpoint logic.
                //
                // This function sends the request to the transport layer, handling query parameters,
                // input, and path selection. It returns the response or an error.
                //
                // # Arguments
                //
                // * `transport` - A reference to the transport layer for sending requests.
                // * `timeout` - An optional timeout for the request.
                //
                // # Returns
                //
                // A `Result` containing the response or an error.
                async fn execute(&self) -> Result<TransportArgs, error::EscliError> {
                    // TODO: restrict the generation to endpoints with actual query params.
                    #[derive(serde::Serialize)]
                    struct Q {
                        $(for field in &self.query_parameters =>
                            $(&field.name()): $(&field.typ()),$['\r']
                        )
                    }

                    let q = Q {
                        $(for field in &self.query_parameters =>
                            $(&field.name()): self.$(&field.name())$(&field.clone_candidate()),$['\r']
                        )
                    };

                    $(self.input_handling())

                    let mut headers = HeaderMap::new();
                    for (k, v) in &self.header {
                        if let (Ok(header_name), Ok(header_value)) = (
                            elasticsearch::http::headers::HeaderName::from_bytes(k.as_bytes()),
                            elasticsearch::http::headers::HeaderValue::from_str(v),
                        ) {
                            headers.insert(header_name, header_value);
                        }
                    }

                    $(self.paths_selection)

                    Ok(TransportArgs {
                        method,
                        path: url,
                        headers,
                        query_string: Box::new(q),
                        body: $(if self.has_request {
                                Some(body)
                            } else {
                                Option::<String>::None
                        }),
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::Field;
    use crate::path_parameter::PathParameter;
    use std::collections::HashSet;

    #[test]
    fn test_collect_optional_parameters() {
        let endpoint = Endpoint {
            e: clients_schema::Endpoint {
                name: "test.endpoint".to_string(),
                description: String::new(),
                doc_url: None,
                doc_id: None,
                ext_doc_id: None,
                ext_doc_url: None,
                deprecation: None,
                availability: None,
                urls: vec![],
                request_media_type: vec![],
                response_media_type: vec![],
                request: None,
                request_body_required: false,
                doc_tag: None,
                response: None,
                privileges: None,
            },
            path_parameters: vec![
                Field::new(
                    "foo".to_string(),
                    "".to_string(),
                    true,
                    "String".to_string(),
                    None,
                ),
                Field::new(
                    "bar".to_string(),
                    "".to_string(),
                    false,
                    "String".to_string(),
                    None,
                ),
            ],
            query_parameters: vec![],
            enums: HashMap::new(),
            paths_selection: Tokens::new(),
            has_request: false,
        };
        let optional = endpoint.collect_optional_parameters();
        let mut expected = HashSet::new();
        expected.insert("bar".to_string());
        assert_eq!(optional, expected);
    }

    #[test]
    fn test_build_path_parameters() {
        let mut endpoint = Endpoint {
            e: clients_schema::Endpoint {
                name: "test.endpoint".to_string(),
                description: String::new(),
                doc_url: None,
                doc_id: None,
                ext_doc_id: None,
                ext_doc_url: None,
                deprecation: None,
                availability: None,
                urls: vec![clients_schema::UrlTemplate {
                    path: "/foo/{bar}/{baz}".to_string(),
                    methods: vec!["GET".to_string()],
                    deprecation: None,
                }],
                request_media_type: vec![],
                response_media_type: vec![],
                request: None,
                request_body_required: false,
                doc_tag: None,
                response: None,
                privileges: None,
            },
            path_parameters: vec![Field::new(
                "bar".to_string(),
                "".to_string(),
                true,
                "String".to_string(),
                None,
            )],
            query_parameters: vec![],
            enums: HashMap::new(),
            paths_selection: Tokens::new(),
            has_request: false,
        };
        let optional = HashSet::new();
        let params = endpoint.build_path_parameters(&optional);
        assert_eq!(params.len(), 1);
        let param = &params[0];
        assert_eq!(param.path(), "/foo/{bar}/{baz}");
        assert!(param.params().contains(&"baz".to_string()));
    }

    #[test]
    fn test_generate_path_selection_tokens_single() {
        let mut toks = Tokens::new();
        let path_param = PathParameter::new(
            "/foo/{bar}".to_string(),
            vec!["bar".to_string()],
            HashSet::from(["bar".to_string()]),
            HashSet::new(),
            "Get".to_string(),
        );
        let path_params = vec![path_param];
        let endpoint = Endpoint {
            e: clients_schema::Endpoint {
                name: "test.endpoint".to_string(),
                description: String::new(),
                doc_url: None,
                doc_id: None,
                ext_doc_id: None,
                ext_doc_url: None,
                deprecation: None,
                availability: None,
                urls: vec![],
                request_media_type: vec![],
                response_media_type: vec![],
                request: None,

                request_body_required: false,
                doc_tag: None,
                response: None,
                privileges: None,
            },
            path_parameters: vec![],
            query_parameters: vec![],
            enums: HashMap::new(),
            paths_selection: Tokens::new(),
            has_request: false,
        };
        endpoint.generate_path_selection_tokens(&mut toks, &path_params);
        let toks_str = toks.to_string().unwrap_or_default();
        assert!(toks_str.contains("let url"));
        assert!(toks_str.contains("let method"));
    }
}
