use crate::{
    edits::{self, AnnotatedEdit, ParsedDocument},
    server::lsp_project::LspProject,
};
use std::{collections::HashMap, str::FromStr};

const UNKNOWN_VARIABLE: &str = "aiken::check::unknown::variable";
const UNKNOWN_TYPE: &str = "aiken::check::unknown::type";
const UNKNOWN_CONSTRUCTOR: &str = "aiken::check::unknown::type_constructor";
const UNKNOWN_MODULE: &str = "aiken::check::unknown::module";
const UNUSED_IMPORT_VALUE: &str = "aiken::check::unused:import::value";
const UNUSED_IMPORT_MODULE: &str = "aiken::check::unused::import::module";
const USE_LET: &str = "aiken::check::single_constructor_expect";
const UNUSED_RECORD_FIELDS: &str = "aiken::check::syntax::unused_record_fields";
const UTF8_BYTE_ARRAY_IS_VALID_HEX_STRING: &str =
    "aiken::check::syntax::bytearray_literal_is_hex_string";
const UNEXPECTED_TYPE_HOLE: &str = "aiken::check::unexpected::type_hole";
const UNUSED_PRIVATE_FUNCTION: &str = "aiken::check::unused::function";
const UNUSED_PRIVATE_CONSTANT: &str = "aiken::check::unused::constant";
const UNUSED_PRIVATE_TYPE: &str = "aiken::check::unused::type";
const PRIVATE_TYPE_LEAK: &str = "aiken::check::private_leak";
const UNUSED_VARIABLE: &str = "aiken::check::unused::variable";
const DISCARDED_LET_ASSIGNMENT: &str = "aiken::check::unused::discarded_let_assignment";
const TODO: &str = "aiken::check::todo";
const IF_IS_ON_NON_DATA: &str = "aiken::check::if_is_on_non_data";
const SINGLE_WHEN_CLAUSE: &str = "aiken::check::single_when_clause";
const IMPLICIT_DISCARD: &str = "aiken::check::implicit_discard";
const ALL_FIELDS_RECORD_UPDATE: &str = "aiken::check::record_update::all_fields";
const NO_FIELDS_RECORD_UPDATE: &str = "aiken::check::record_update::no_fields";

/// Errors for which we can provide quickfixes
#[allow(clippy::enum_variant_names)]
pub enum Quickfix {
    UnknownIdentifier(lsp_types::Diagnostic),
    UnknownModule(lsp_types::Diagnostic),
    UnknownConstructor(lsp_types::Diagnostic),
    UnusedImports(Vec<lsp_types::Diagnostic>),
    Utf8ByteArrayIsValidHexString(lsp_types::Diagnostic),
    UseLet(lsp_types::Diagnostic),
    UnusedRecordFields(lsp_types::Diagnostic),
    UnexpectedTypeHole(lsp_types::Diagnostic),
    UnusedPrivate(lsp_types::Diagnostic),
    PrivateLeak(lsp_types::Diagnostic),
    UnusedVariable(lsp_types::Diagnostic),
    DiscardedLetAssignment(lsp_types::Diagnostic),
    Todo(lsp_types::Diagnostic),
    IfIsOnNonData(lsp_types::Diagnostic),
    SingleWhenClause(lsp_types::Diagnostic),
    ImplicitDiscard(lsp_types::Diagnostic),
    AllFieldsRecordUpdate(lsp_types::Diagnostic),
    NoFieldsRecordUpdate(lsp_types::Diagnostic),
}

fn match_code(
    diagnostic: &lsp_types::Diagnostic,
    severity: lsp_types::DiagnosticSeverity,
    expected: &str,
) -> bool {
    diagnostic.code == Some(lsp_types::NumberOrString::String(expected.to_string()))
        && diagnostic.severity == Some(severity)
}

/// Assert whether a diagnostic can be automatically fixed. Note that diagnostics often comes in
/// two severities, an error and hint; so we must be careful only addressing errors.
pub fn assert(diagnostic: lsp_types::Diagnostic) -> Option<Quickfix> {
    use lsp_types::DiagnosticSeverity as Severity;

    if match_code(&diagnostic, Severity::ERROR, UNKNOWN_VARIABLE)
        || match_code(&diagnostic, Severity::ERROR, UNKNOWN_TYPE)
    {
        return Some(Quickfix::UnknownIdentifier(diagnostic));
    }

    if match_code(&diagnostic, Severity::ERROR, UNKNOWN_CONSTRUCTOR) {
        return Some(Quickfix::UnknownConstructor(diagnostic));
    }

    if match_code(&diagnostic, Severity::ERROR, UNKNOWN_MODULE) {
        return Some(Quickfix::UnknownModule(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, UNUSED_IMPORT_VALUE)
        || match_code(&diagnostic, Severity::WARNING, UNUSED_IMPORT_MODULE)
    {
        return Some(Quickfix::UnusedImports(vec![diagnostic]));
    }

    if match_code(
        &diagnostic,
        Severity::WARNING,
        UTF8_BYTE_ARRAY_IS_VALID_HEX_STRING,
    ) {
        return Some(Quickfix::Utf8ByteArrayIsValidHexString(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, USE_LET) {
        return Some(Quickfix::UseLet(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, UNUSED_RECORD_FIELDS) {
        return Some(Quickfix::UnusedRecordFields(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, UNEXPECTED_TYPE_HOLE) {
        return Some(Quickfix::UnexpectedTypeHole(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, UNUSED_PRIVATE_FUNCTION)
        || match_code(&diagnostic, Severity::WARNING, UNUSED_PRIVATE_CONSTANT)
        || match_code(&diagnostic, Severity::WARNING, UNUSED_PRIVATE_TYPE)
    {
        return Some(Quickfix::UnusedPrivate(diagnostic));
    }

    if match_code(&diagnostic, Severity::ERROR, PRIVATE_TYPE_LEAK) {
        return Some(Quickfix::PrivateLeak(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, UNUSED_VARIABLE) {
        return Some(Quickfix::UnusedVariable(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, DISCARDED_LET_ASSIGNMENT) {
        return Some(Quickfix::DiscardedLetAssignment(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, TODO) {
        return Some(Quickfix::Todo(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, IF_IS_ON_NON_DATA) {
        return Some(Quickfix::IfIsOnNonData(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, SINGLE_WHEN_CLAUSE) {
        return Some(Quickfix::SingleWhenClause(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, IMPLICIT_DISCARD) {
        return Some(Quickfix::ImplicitDiscard(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, ALL_FIELDS_RECORD_UPDATE) {
        return Some(Quickfix::AllFieldsRecordUpdate(diagnostic));
    }

    if match_code(&diagnostic, Severity::WARNING, NO_FIELDS_RECORD_UPDATE) {
        return Some(Quickfix::NoFieldsRecordUpdate(diagnostic));
    }

    None
}

pub fn quickfix(
    compiler: &LspProject,
    text_document: &lsp_types::TextDocumentIdentifier,
    quickfix: &Quickfix,
    edited: &HashMap<String, String>,
) -> Vec<lsp_types::CodeAction> {
    let mut actions = Vec::new();

    // Try to parse document - only needed for import-based actions
    let parsed_document = edits::parse_document(text_document, edited);

    match quickfix {
        // --- Actions that NEED parsed_document for AST manipulation ---
        Quickfix::UnknownIdentifier(diagnostic) => {
            if let Some(ref parsed) = parsed_document {
                each_as_distinct_action(
                    &mut actions,
                    text_document,
                    diagnostic,
                    unknown_identifier(
                        compiler,
                        parsed,
                        &diagnostic.range,
                        diagnostic.data.as_ref(),
                    ),
                );
            }
        }
        Quickfix::UnknownModule(diagnostic) => {
            if let Some(ref parsed) = parsed_document {
                each_as_distinct_action(
                    &mut actions,
                    text_document,
                    diagnostic,
                    unknown_module(compiler, parsed, diagnostic.data.as_ref()),
                );
            }
        }
        Quickfix::UnknownConstructor(diagnostic) => {
            if let Some(ref parsed) = parsed_document {
                each_as_distinct_action(
                    &mut actions,
                    text_document,
                    diagnostic,
                    unknown_constructor(
                        compiler,
                        parsed,
                        &diagnostic.range,
                        diagnostic.data.as_ref(),
                    ),
                );
            }
        }
        Quickfix::UnusedImports(diagnostics) => {
            if let Some(ref parsed) = parsed_document {
                as_single_action(
                    &mut actions,
                    text_document,
                    diagnostics.to_owned(),
                    "Remove redundant imports",
                    unused_imports(
                        parsed,
                        diagnostics
                            .iter()
                            .map(|diagnostic| diagnostic.data.as_ref())
                            .collect(),
                    ),
                );
            }
        }
        Quickfix::PrivateLeak(diagnostic) => {
            if let Some(ref parsed) = parsed_document {
                each_as_distinct_action(
                    &mut actions,
                    text_document,
                    diagnostic,
                    make_type_public(parsed, diagnostic),
                );
            }
        }
        // --- Actions that DON'T need parsed_document ---
        Quickfix::Utf8ByteArrayIsValidHexString(diagnostic) => {
            each_as_distinct_action(
                &mut actions,
                text_document,
                diagnostic,
                utf8_byte_array_is_hex_string(diagnostic),
            );
        }
        Quickfix::UseLet(diagnostic) => {
            each_as_distinct_action(&mut actions, text_document, diagnostic, use_let(diagnostic));
        }
        Quickfix::UnusedRecordFields(diagnostic) => {
            each_as_distinct_action(
                &mut actions,
                text_document,
                diagnostic,
                unused_record_fields(diagnostic),
            );
        }
        Quickfix::UnexpectedTypeHole(diagnostic) => {
            each_as_distinct_action(
                &mut actions,
                text_document,
                diagnostic,
                fill_type_hole(diagnostic),
            );
        }
        Quickfix::UnusedPrivate(diagnostic) => {
            each_as_distinct_action(
                &mut actions,
                text_document,
                diagnostic,
                make_value_public(diagnostic),
            );
        }
        // --- NEW simple text-replacement actions ---
        Quickfix::UnusedVariable(diagnostic) => {
            each_as_distinct_action(
                &mut actions,
                text_document,
                diagnostic,
                prefix_with_underscore(diagnostic),
            );
        }
        Quickfix::DiscardedLetAssignment(diagnostic) => {
            each_as_distinct_action(
                &mut actions,
                text_document,
                diagnostic,
                prefix_with_underscore(diagnostic),
            );
        }
        Quickfix::Todo(diagnostic) => {
            each_as_distinct_action(
                &mut actions,
                text_document,
                diagnostic,
                fill_todo(diagnostic),
            );
        }
        Quickfix::IfIsOnNonData(diagnostic) => {
            each_as_distinct_action(
                &mut actions,
                text_document,
                diagnostic,
                replace_if_with_when(diagnostic),
            );
        }
        Quickfix::SingleWhenClause(diagnostic) => {
            each_as_distinct_action(
                &mut actions,
                text_document,
                diagnostic,
                replace_when_with_let(diagnostic),
            );
        }
        Quickfix::ImplicitDiscard(diagnostic) => {
            each_as_distinct_action(
                &mut actions,
                text_document,
                diagnostic,
                add_discard_binding(diagnostic),
            );
        }
        Quickfix::AllFieldsRecordUpdate(diagnostic) => {
            each_as_distinct_action(
                &mut actions,
                text_document,
                diagnostic,
                remove_redundant_record_update(diagnostic),
            );
        }
        Quickfix::NoFieldsRecordUpdate(diagnostic) => {
            each_as_distinct_action(
                &mut actions,
                text_document,
                diagnostic,
                remove_redundant_record_update(diagnostic),
            );
        }
    }

    actions
}

fn each_as_distinct_action(
    actions: &mut Vec<lsp_types::CodeAction>,
    text_document: &lsp_types::TextDocumentIdentifier,
    diagnostic: &lsp_types::Diagnostic,
    edits: Vec<AnnotatedEdit>,
) {
    for (i, edit) in edits.into_iter().enumerate() {
        let mut changes = HashMap::new();

        let title = match edit {
            AnnotatedEdit::SimpleEdit(title, one) => {
                changes.insert(text_document.uri.clone(), vec![one]);
                title
            }
            AnnotatedEdit::CombinedEdits(title, many) => {
                changes.insert(text_document.uri.clone(), many);
                title
            }
        };

        actions.push(lsp_types::CodeAction {
            title,
            kind: Some(lsp_types::CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            is_preferred: if i == 0 { Some(true) } else { None },
            disabled: None,
            data: None,
            command: None,
            edit: Some(lsp_types::WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            }),
        });
    }
}

fn as_single_action(
    actions: &mut Vec<lsp_types::CodeAction>,
    text_document: &lsp_types::TextDocumentIdentifier,
    diagnostics: Vec<lsp_types::Diagnostic>,
    title: &str,
    edits: Vec<AnnotatedEdit>,
) {
    let mut changes = HashMap::new();

    changes.insert(
        text_document.uri.clone(),
        edits
            .into_iter()
            .flat_map(|edit| match edit {
                AnnotatedEdit::SimpleEdit(_, one) => vec![one],
                AnnotatedEdit::CombinedEdits(_, many) => many,
            })
            .collect(),
    );

    actions.push(lsp_types::CodeAction {
        title: title.to_string(),
        kind: Some(lsp_types::CodeActionKind::QUICKFIX),
        diagnostics: Some(diagnostics),
        is_preferred: Some(true),
        disabled: None,
        data: None,
        command: None,
        edit: Some(lsp_types::WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
    });
}

fn unknown_identifier(
    compiler: &LspProject,
    parsed_document: &ParsedDocument,
    range: &lsp_types::Range,
    data: Option<&serde_json::Value>,
) -> Vec<AnnotatedEdit> {
    let mut edits = Vec::new();

    if let Some(serde_json::Value::String(var_name)) = data {
        for module in compiler.project.glossary().find_definition(var_name) {
            if let Some(edit) = parsed_document.import(module, Some(var_name)) {
                edits.push(edit)
            }

            if let Some(edit) = suggest_qualified(parsed_document, module, var_name, range) {
                edits.push(edit)
            }
        }
    }

    edits
}

fn unknown_constructor(
    compiler: &LspProject,
    parsed_document: &ParsedDocument,
    range: &lsp_types::Range,
    data: Option<&serde_json::Value>,
) -> Vec<AnnotatedEdit> {
    let mut edits = Vec::new();

    if let Some(serde_json::Value::String(constructor_name)) = data {
        for module in compiler
            .project
            .glossary()
            .find_constructor(constructor_name)
        {
            if let Some(edit) = parsed_document.import(module, Some(constructor_name)) {
                edits.push(edit)
            }

            if let Some(edit) = suggest_qualified(parsed_document, module, constructor_name, range)
            {
                edits.push(edit)
            }
        }
    }

    edits
}

fn suggest_qualified(
    parsed_document: &ParsedDocument,
    module: &str,
    identifier: &str,
    range: &lsp_types::Range,
) -> Option<AnnotatedEdit> {
    if let Some(AnnotatedEdit::SimpleEdit(use_qualified_title, use_qualified)) =
        parsed_document.use_qualified(module, identifier, range)
    {
        if let Some(AnnotatedEdit::SimpleEdit(_, add_new_line)) =
            parsed_document.import(module, None)
        {
            return Some(AnnotatedEdit::CombinedEdits(
                use_qualified_title,
                vec![add_new_line, use_qualified],
            ));
        } else {
            return Some(AnnotatedEdit::SimpleEdit(
                use_qualified_title,
                use_qualified,
            ));
        }
    }

    None
}

fn unknown_module(
    compiler: &LspProject,
    parsed_document: &ParsedDocument,
    data: Option<&serde_json::Value>,
) -> Vec<AnnotatedEdit> {
    let mut edits = Vec::new();

    if let Some(serde_json::Value::String(module_name)) = data
        && let Some(module) = compiler.project.glossary().find_module(module_name)
        && let Some(edit) = parsed_document.import(module, None)
    {
        edits.push(edit);
    }

    edits
}

fn unused_imports(
    parsed_document: &ParsedDocument,
    datas: Vec<Option<&serde_json::Value>>,
) -> Vec<AnnotatedEdit> {
    let mut edits = Vec::new();

    for data in datas.iter().rev().flatten() {
        if let serde_json::Value::String(args) = data {
            let args = args.split(',').collect::<Vec<&str>>();
            match args.as_slice() {
                &[is_qualified, start] => {
                    let start = start
                        .parse::<usize>()
                        .expect("malformed unused_imports argument: not a usize");

                    let is_qualified = FromStr::from_str(is_qualified)
                        .expect("malformed unused_imports argument: not a bool");

                    edits.push(parsed_document.remove_import(start, is_qualified));
                }
                _ => {
                    panic!("malformed unused_imports arguments: not a 2-tuple");
                }
            }
        }
    }

    edits
}

fn utf8_byte_array_is_hex_string(diagnostic: &lsp_types::Diagnostic) -> Vec<AnnotatedEdit> {
    let mut edits = Vec::new();

    if let Some(serde_json::Value::String(value)) = diagnostic.data.as_ref() {
        edits.push(AnnotatedEdit::SimpleEdit(
            "Prefix with #".to_string(),
            lsp_types::TextEdit {
                range: diagnostic.range,
                new_text: format!("#\"{value}\""),
            },
        ))
    }

    edits
}

fn use_let(diagnostic: &lsp_types::Diagnostic) -> Vec<AnnotatedEdit> {
    vec![AnnotatedEdit::SimpleEdit(
        "Use 'let' instead of 'expect'".to_string(),
        lsp_types::TextEdit {
            range: diagnostic.range,
            new_text: "let".to_string(),
        },
    )]
}

fn unused_record_fields(diagnostic: &lsp_types::Diagnostic) -> Vec<AnnotatedEdit> {
    let mut edits = Vec::new();

    if let Some(serde_json::Value::String(new_text)) = diagnostic.data.as_ref() {
        edits.push(AnnotatedEdit::SimpleEdit(
            "Destructure using named fields".to_string(),
            lsp_types::TextEdit {
                range: diagnostic.range,
                new_text: new_text.clone(),
            },
        ));
    }

    edits
}

fn fill_type_hole(diagnostic: &lsp_types::Diagnostic) -> Vec<AnnotatedEdit> {
    let mut edits = Vec::new();

    if let Some(serde_json::Value::String(inferred_type)) = diagnostic.data.as_ref() {
        edits.push(AnnotatedEdit::SimpleEdit(
            format!("Pluck '{inferred_type}'"),
            lsp_types::TextEdit {
                range: diagnostic.range,
                new_text: inferred_type.to_string(),
            },
        ));
    }

    edits
}

fn make_value_public(diagnostic: &lsp_types::Diagnostic) -> Vec<AnnotatedEdit> {
    let mut edits = Vec::new();

    if let Some(serde_json::Value::String(name)) = diagnostic.data.as_ref() {
        edits.push(AnnotatedEdit::SimpleEdit(
            format!("Make '{name}' public"),
            lsp_types::TextEdit {
                range: lsp_types::Range {
                    start: diagnostic.range.start,
                    end: diagnostic.range.start,
                },
                new_text: "pub ".to_string(),
            },
        ));
    }

    edits
}

fn make_type_public(
    parsed_document: &ParsedDocument,
    diagnostic: &lsp_types::Diagnostic,
) -> Vec<AnnotatedEdit> {
    let mut edits = Vec::new();

    if let Some(serde_json::Value::String(args)) = diagnostic.data.as_ref() {
        let args = args.split(',').collect::<Vec<&str>>();
        match args.as_slice() {
            &[name, start] => {
                let start = parsed_document.position(
                    start
                        .parse::<usize>()
                        .expect("malformed private_leak argument: not a usize"),
                );

                edits.push(AnnotatedEdit::SimpleEdit(
                    format!("Make '{name}' public"),
                    lsp_types::TextEdit {
                        range: lsp_types::Range { start, end: start },
                        new_text: "pub ".to_string(),
                    },
                ));
            }
            _ => {
                panic!("malformed private_leak arguments: not a 2-tuple");
            }
        }
    }

    edits
}

fn prefix_with_underscore(diagnostic: &lsp_types::Diagnostic) -> Vec<AnnotatedEdit> {
    vec![AnnotatedEdit::SimpleEdit(
        "Prefix with '_'".to_string(),
        lsp_types::TextEdit {
            range: lsp_types::Range {
                start: diagnostic.range.start,
                end: diagnostic.range.start,
            },
            new_text: "_".to_string(),
        },
    )]
}

fn fill_todo(diagnostic: &lsp_types::Diagnostic) -> Vec<AnnotatedEdit> {
    let mut edits = Vec::new();
    if let Some(serde_json::Value::String(inferred_type)) = diagnostic.data.as_ref() {
        edits.push(AnnotatedEdit::SimpleEdit(
            format!("Replace 'todo' with '{inferred_type}'"),
            lsp_types::TextEdit {
                range: diagnostic.range,
                new_text: inferred_type.clone(),
            },
        ));
    }
    edits.push(AnnotatedEdit::SimpleEdit(
        "Replace 'todo' with '_'".to_string(),
        lsp_types::TextEdit {
            range: diagnostic.range,
            new_text: "_".to_string(),
        },
    ));
    edits
}

fn replace_if_with_when(diagnostic: &lsp_types::Diagnostic) -> Vec<AnnotatedEdit> {
    vec![AnnotatedEdit::SimpleEdit(
        "Replace 'if' with 'when'".to_string(),
        lsp_types::TextEdit {
            range: diagnostic.range,
            new_text: "when".to_string(),
        },
    )]
}

fn replace_when_with_let(diagnostic: &lsp_types::Diagnostic) -> Vec<AnnotatedEdit> {
    let mut edits = Vec::new();
    if let Some(serde_json::Value::String(let_binding)) = diagnostic.data.as_ref() {
        edits.push(AnnotatedEdit::SimpleEdit(
            "Replace 'when' with 'let'".to_string(),
            lsp_types::TextEdit {
                range: diagnostic.range,
                new_text: let_binding.clone(),
            },
        ));
    }
    edits
}

fn add_discard_binding(diagnostic: &lsp_types::Diagnostic) -> Vec<AnnotatedEdit> {
    vec![AnnotatedEdit::SimpleEdit(
        "Add explicit discard binding".to_string(),
        lsp_types::TextEdit {
            range: lsp_types::Range {
                start: diagnostic.range.start,
                end: diagnostic.range.start,
            },
            new_text: "let _ = ".to_string(),
        },
    )]
}

fn remove_redundant_record_update(diagnostic: &lsp_types::Diagnostic) -> Vec<AnnotatedEdit> {
    vec![AnnotatedEdit::SimpleEdit(
        "Remove redundant record update".to_string(),
        lsp_types::TextEdit {
            range: diagnostic.range,
            new_text: String::new(),
        },
    )]
}
