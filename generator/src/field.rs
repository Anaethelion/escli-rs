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

use genco::tokens::quoted;
use genco::{Tokens, quote};

// Represents a field in an API endpoint.
// A field contains metadata such as its name, description, type, and whether it is required.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    // The name of the field.
    name: String,
    // A description of the field.
    description: String,
    // Indicates whether the field is required.
    required: bool,
    // The type of the field.
    ty: String,
    // An optional default value for the field.
    default_value: Option<String>,
}

impl Field {
    // Creates a new `Field` instance.
    //
    // # Arguments
    //
    // * `name` - The name of the field.
    // * `description` - A description of the field.
    // * `required` - Whether the field is required.
    // * `ty` - The type of the field.
    //
    // # Returns
    //
    // A new `Field` instance.
    pub fn new(
        name: String,
        description: String,
        required: bool,
        ty: String,
        default_value: Option<String>,
    ) -> Self {
        let name = Self::sanitize_field_name(&name);

        let description = if description.is_empty() {
            "".to_string()
        } else {
            description.to_string()
        };

        Field {
            name,
            description,
            required,
            ty,
            default_value,
        }
    }

    // Returns the type of the field, wrapped in `Option` if the field is not required.
    pub fn typ(&self) -> String {
        if self.required {
            self.ty.clone()
        } else {
            format!("Option<{}>", self.ty)
        }
    }

    pub fn clone_candidate(&self) -> Tokens {
        match self.ty.as_str() {
            "String" => {
                quote! {
                    .clone()
                }
            }
            &_ => {
                quote! {}
            }
        }
    }

    // Returns the name of the field.
    pub fn name(&self) -> String {
        self.name.clone()
    }

    fn sanitize_field_name(name: &str) -> String {
        match name {
            "type" => "ty".to_string(),
            "help" => "help_".to_string(),
            "h" => "h_".to_string(),
            // Add more reserved words as needed
            _ => name.to_string(),
        }
    }

    pub(crate) fn original_field_name(&self) -> String {
        match self.name.as_str() {
            "ty" => "r#type".to_string(),
            "help_" => "help".to_string(),
            "h_" => "h".to_string(),
            _ => self.name.to_string(),
        }
    }

    // Returns if the field is required.
    pub fn required(&self) -> bool {
        self.required
    }

    // Returns the short help text, which is the first sentence of the description.
    pub fn short_help(&self) -> String {
        self.description.lines().next().unwrap_or("").to_string()
    }

    // Returns the long help text, which is the full description.
    pub fn long_help(&self) -> String {
        self.description.clone().to_string()
    }

    // Generates the argument definition for the field in a CLI command.
    //
    // # Returns
    //
    // A `Tokens` object representing the argument definition.
    pub fn arg(&self) -> Tokens {
        let short_help = self.short_help().escape_default().to_string();
        let long_help = self.long_help().escape_default().to_string();
        let name = self.name.escape_default().to_string();

        let base_quote = |action: Option<&str>| match action {
            Some(action) => quote! {
                #[arg(long($(quoted(&name))), help = $(quoted(&short_help)), long_help = $(quoted(&long_help)), action=$(action))]
                $(&self.name): $(&self.typ()),$['\r']
            },
            None => quote! {
                #[arg(long($(quoted(&name))), help = $(quoted(&short_help)), long_help = $(quoted(&long_help)))]
                $(&self.name): $(&self.typ()),$['\r']
            },
        };

        if self.required {
            match self.ty.as_str() {
                "bool" => base_quote(None),
                _ => quote! {
                    #[arg(help = $(quoted(&short_help)), long_help = $(quoted(&long_help)))]
                    $(&self.name): $(&self.typ()),$['\r']
                },
            }
        } else {
            match self.ty.as_str() {
                "bool" => {
                    if let Some(default_value) = &self.default_value {
                        match default_value.as_str() {
                            "false" => base_quote(Some("clap::ArgAction::SetTrue")),
                            "true" => base_quote(Some("clap::ArgAction::SetFalse")),
                            _ => base_quote(None),
                        }
                    } else {
                        base_quote(None)
                    }
                }
                _ => base_quote(None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_help_returns_first_line_of_description() {
        let field = Field {
            name: "example".to_string(),
            description: "First line.\nSecond line.".to_string(),
            required: true,
            ty: "String".to_string(),
            default_value: None,
        };
        assert_eq!(field.short_help(), "First line.");
    }

    #[test]
    fn short_help_returns_empty_string_when_description_is_empty() {
        let field = Field {
            name: "example".to_string(),
            description: "".to_string(),
            required: true,
            ty: "String".to_string(),
            default_value: None,
        };
        assert_eq!(field.short_help(), "");
    }

    #[test]
    fn short_help_handles_single_line_description() {
        let field = Field {
            name: "example".to_string(),
            description: "Single line description.".to_string(),
            required: true,
            ty: "String".to_string(),
            default_value: None,
        };
        assert_eq!(field.short_help(), "Single line description.");
    }

    #[test]
    fn long_help_returns_full_description() {
        let field = Field {
            name: "example".to_string(),
            description: "Full description text.".to_string(),
            required: true,
            ty: "String".to_string(),
            default_value: None,
        };
        assert_eq!(field.long_help(), "Full description text.");
    }

    #[test]
    fn long_help_returns_empty_string_when_description_is_empty() {
        let field = Field {
            name: "example".to_string(),
            description: "".to_string(),
            required: true,
            ty: "String".to_string(),
            default_value: None,
        };
        assert_eq!(field.long_help(), "");
    }

    #[test]
    fn long_help_handles_multiline_description() {
        let field = Field {
            name: "example".to_string(),
            description: "Line one.\nLine two.\nLine three.".to_string(),
            required: true,
            ty: "String".to_string(),
            default_value: None,
        };
        assert_eq!(field.long_help(), "Line one.\nLine two.\nLine three.");
    }

    #[test]
    fn arg_generates_correct_tokens_for_required_bool_field() {
        let field = Field {
            name: "flag".to_string(),
            description: "A boolean flag.".to_string(),
            required: true,
            ty: "bool".to_string(),
            default_value: None,
        };
        let tokens = field.arg().to_string().unwrap_or_default();
        assert!(
            tokens.contains(
                "#[arg(long, help = \"A boolean flag.\", long_help = \"A boolean flag.\")]"
            )
        );
        assert!(tokens.contains("flag: bool,"));
    }

    #[test]
    fn arg_generates_correct_tokens_for_required_non_bool_field() {
        let field = Field {
            name: "value".to_string(),
            description: "A required value.".to_string(),
            required: true,
            ty: "String".to_string(),
            default_value: None,
        };
        let tokens = field.arg().to_string().unwrap_or_default();
        assert!(
            tokens.contains(
                "#[arg(help = \"A required value.\", long_help = \"A required value.\")]"
            )
        );
        assert!(tokens.contains("value: String,"));
    }

    #[test]
    fn arg_generates_correct_tokens_for_optional_field() {
        let field = Field {
            name: "optional_value".to_string(),
            description: "An optional value.".to_string(),
            required: false,
            ty: "String".to_string(),
            default_value: None,
        };
        let tokens = field.arg().to_string().unwrap_or_default();
        assert!(tokens.contains(
            "#[arg(long, help = \"An optional value.\", long_help = \"An optional value.\")]"
        ));
        assert!(tokens.contains("optional_value: Option<String>,"));
    }

    #[test]
    fn arg_handles_empty_description_correctly() {
        let field = Field {
            name: "empty_desc".to_string(),
            description: "".to_string(),
            required: true,
            ty: "String".to_string(),
            default_value: None,
        };
        let tokens = field.arg().to_string().unwrap_or_default();
        assert!(tokens.contains("#[arg(help = \"\", long_help = \"\")]"));
        assert!(tokens.contains("empty_desc: String,"));
    }

    #[test]
    fn typ_returns_original_type_when_field_is_required() {
        let field = Field {
            name: "example".to_string(),
            description: "A required field.".to_string(),
            required: true,
            ty: "String".to_string(),
            default_value: None,
        };
        assert_eq!(field.typ(), "String");
    }

    #[test]
    fn typ_returns_option_wrapped_type_when_field_is_not_required() {
        let field = Field {
            name: "example".to_string(),
            description: "An optional field.".to_string(),
            required: false,
            ty: "String".to_string(),
            default_value: None,
        };
        assert_eq!(field.typ(), "Option<String>");
    }

    #[test]
    fn typ_handles_empty_type_correctly() {
        let field = Field {
            name: "example".to_string(),
            description: "A field with no type.".to_string(),
            required: true,
            ty: "".to_string(),
            default_value: None,
        };
        assert_eq!(field.typ(), "");
    }

    #[test]
    fn typ_handles_non_standard_type_correctly() {
        let field = Field {
            name: "example".to_string(),
            description: "A field with a custom type.".to_string(),
            required: false,
            ty: "CustomType".to_string(),
            default_value: None,
        };
        assert_eq!(field.typ(), "Option<CustomType>");
    }

    #[test]
    fn arg_optional_bool_with_default_false_sets_settrue_action() {
        let field = Field {
            name: "flag".to_string(),
            description: "Optional flag.".to_string(),
            required: false,
            ty: "bool".to_string(),
            default_value: Some("false".to_string()),
        };
        let tokens = field.arg().to_string().unwrap_or_default();
        assert!(tokens.contains("action=clap::ArgAction::SetTrue"));
        assert!(tokens.contains("flag: Option<bool>,"));
    }

    #[test]
    fn arg_optional_bool_with_default_true_sets_setfalse_action() {
        let field = Field {
            name: "flag".to_string(),
            description: "Optional flag.".to_string(),
            required: false,
            ty: "bool".to_string(),
            default_value: Some("true".to_string()),
        };
        let tokens = field.arg().to_string().unwrap_or_default();
        assert!(tokens.contains("action=clap::ArgAction::SetFalse"));
        assert!(tokens.contains("flag: Option<bool>,"));
    }

    #[test]
    fn arg_optional_bool_with_nonstandard_default_omits_action() {
        let field = Field {
            name: "flag".to_string(),
            description: "Optional flag.".to_string(),
            required: false,
            ty: "bool".to_string(),
            default_value: Some("maybe".to_string()),
        };
        let tokens = field.arg().to_string().unwrap_or_default();
        assert!(!tokens.contains("action=clap::ArgAction::SetTrue"));
        assert!(!tokens.contains("action=clap::ArgAction::SetFalse"));
        assert!(tokens.contains("flag: Option<bool>,"));
    }

    #[test]
    fn arg_optional_bool_with_no_default_omits_action() {
        let field = Field {
            name: "flag".to_string(),
            description: "Optional flag.".to_string(),
            required: false,
            ty: "bool".to_string(),
            default_value: None,
        };
        let tokens = field.arg().to_string().unwrap_or_default();
        assert!(!tokens.contains("action=clap::ArgAction::SetTrue"));
        assert!(!tokens.contains("action=clap::ArgAction::SetFalse"));
        assert!(tokens.contains("flag: Option<bool>,"));
    }
}
