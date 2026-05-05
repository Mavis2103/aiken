use crate::server::Server;
use crate::utils::span_to_lsp_range;
use aiken_lang::ast::{Definition, TypedDefinition};
use aiken_lang::line_numbers::LineNumbers;
use lsp_types::{Location, SymbolInformation, SymbolKind, WorkspaceSymbolResponse};

impl Server {
    /// Implements the `workspace/symbol` request.
    /// Returns all symbols across all compiled modules that match the given query.
    /// An empty query returns all symbols.
    pub fn workspace_symbol(&self, query: String) -> Option<WorkspaceSymbolResponse> {
        let compiler = self.compiler.as_ref()?;
        let mut symbols = Vec::new();

        for (module_name, checked_module) in &compiler.modules {
            let uri = self.module_name_to_uri(module_name)?;
            let line_numbers = LineNumbers::new(&checked_module.code);

            for definition in &checked_module.ast.definitions {
                collect_definition_symbols(definition, &query, &uri, &line_numbers, &mut symbols);
            }
        }

        if symbols.is_empty() {
            None
        } else {
            Some(WorkspaceSymbolResponse::Flat(symbols))
        }
    }
}

/// Collect LSP SymbolInformation entries from a single typed definition.
///
/// Maps Aiken constructs to `SymbolKind`:
///   - `Fn`, `Test`, `Benchmark` → `FUNCTION`
///   - `DataType`, `TypeAlias`, `Validator` → `CLASS`
///   - `ModuleConstant` → `CONSTANT`
///   - Constructor records (children of DataType) → `CONSTRUCTOR`
///   - `Use` statements → skipped
///
/// Filtering by query is case-insensitive substring matching.
#[allow(deprecated)]
fn collect_definition_symbols(
    definition: &TypedDefinition,
    query: &str,
    uri: &lsp_types::Url,
    line_numbers: &LineNumbers,
    symbols: &mut Vec<SymbolInformation>,
) {
    match definition {
        Definition::Fn(function) => {
            if !matches_query(&function.name, query) {
                return;
            }
            let range = span_to_lsp_range(function.location, line_numbers);
            symbols.push(SymbolInformation {
                name: function.name.clone(),
                kind: SymbolKind::FUNCTION,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                container_name: None,
                tags: None,
            });
        }

        Definition::DataType(data_type) => {
            if matches_query(&data_type.name, query) {
                let range = span_to_lsp_range(data_type.location, line_numbers);
                symbols.push(SymbolInformation {
                    name: data_type.name.clone(),
                    kind: SymbolKind::CLASS,
                    deprecated: None,
                    location: Location {
                        uri: uri.clone(),
                        range,
                    },
                    container_name: None,
                    tags: None,
                });
            }

            for constructor in &data_type.constructors {
                if matches_query(&constructor.name, query) {
                    let range = span_to_lsp_range(constructor.location, line_numbers);
                    symbols.push(SymbolInformation {
                        name: constructor.name.clone(),
                        kind: SymbolKind::CONSTRUCTOR,
                        deprecated: None,
                        location: Location {
                            uri: uri.clone(),
                            range,
                        },
                        container_name: None,
                        tags: None,
                    });
                }
            }
        }

        Definition::TypeAlias(type_alias) => {
            if !matches_query(&type_alias.alias, query) {
                return;
            }
            let range = span_to_lsp_range(type_alias.location, line_numbers);
            symbols.push(SymbolInformation {
                name: type_alias.alias.clone(),
                kind: SymbolKind::CLASS,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                container_name: None,
                tags: None,
            });
        }

        Definition::ModuleConstant(constant) => {
            if !matches_query(&constant.name, query) {
                return;
            }
            let range = span_to_lsp_range(constant.location, line_numbers);
            symbols.push(SymbolInformation {
                name: constant.name.clone(),
                kind: SymbolKind::CONSTANT,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                container_name: None,
                tags: None,
            });
        }

        Definition::Validator(validator) => {
            if !matches_query(&validator.name, query) {
                return;
            }
            let range = span_to_lsp_range(validator.location, line_numbers);
            symbols.push(SymbolInformation {
                name: validator.name.clone(),
                kind: SymbolKind::CLASS,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                container_name: None,
                tags: None,
            });
        }

        Definition::Test(test) => {
            if !matches_query(&test.name, query) {
                return;
            }
            let range = span_to_lsp_range(test.location, line_numbers);
            symbols.push(SymbolInformation {
                name: test.name.clone(),
                kind: SymbolKind::FUNCTION,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                container_name: None,
                tags: None,
            });
        }

        Definition::Benchmark(benchmark) => {
            if !matches_query(&benchmark.name, query) {
                return;
            }
            let range = span_to_lsp_range(benchmark.location, line_numbers);
            symbols.push(SymbolInformation {
                name: benchmark.name.clone(),
                kind: SymbolKind::FUNCTION,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                container_name: None,
                tags: None,
            });
        }

        Definition::Use(_) => {
            // Skip use statements — not meaningful as workspace symbols
        }
    }
}

/// Case-insensitive substring match.
/// An empty query matches everything.
fn matches_query(name: &str, query: &str) -> bool {
    query.is_empty() || name.to_lowercase().contains(&query.to_lowercase())
}
