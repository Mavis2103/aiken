use crate::server::Server;
use aiken_lang::{
    ast::{Definition, Located, Use},
    tipo::pretty::Printer,
    tipo::{TypeInfo, ValueConstructor, ValueConstructorVariant},
};
use aiken_project::module::CheckedModule;
use itertools::Itertools;
use lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
};
use std::collections::HashMap;

impl Server {
    pub fn completion(&self, params: lsp_types::CompletionParams) -> Option<Vec<CompletionItem>> {
        let found = self
            .node_at_position(&params.text_document_position)
            .map(|(_, found)| found);

        let module = self.module_for_uri(&params.text_document_position.text_document.uri);
        let type_info = module.map(|m| &m.ast.type_info);
        let compiler = self.compiler.as_ref();

        match found {
            None => {
                let compiler = compiler?;
                Some(completion_for_import(
                    &compiler.modules,
                    compiler.project.importable_modules(),
                    &[],
                ))
            }
            Some(Located::Definition(Definition::Use(Use { module, .. }))) => {
                let compiler = compiler?;
                Some(completion_for_import(
                    &compiler.modules,
                    compiler.project.importable_modules(),
                    module,
                ))
            }
            Some(Located::Expression(_expression)) => {
                let (compiler, type_info) = match (compiler, type_info) {
                    (Some(c), Some(t)) => (c, t),
                    _ => return None,
                };
                completion_for_expression(type_info, compiler)
            }
            Some(Located::Pattern(_pattern, tipo)) => {
                let type_info = type_info?;
                completion_for_pattern(type_info, &tipo)
            }
            Some(Located::Annotation(_annotation)) => {
                let type_info = type_info?;
                completion_for_annotation(type_info)
            }
            Some(Located::Argument(_arg_name, _tipo)) => {
                let type_info = type_info?;
                completion_for_argument(type_info)
            }
            Some(Located::Definition(_)) => None,
        }
    }
}

/// Generate import path completions from available modules
fn completion_for_import(
    local_modules: &HashMap<String, CheckedModule>,
    dependency_modules: Vec<String>,
    module: &[String],
) -> Vec<CompletionItem> {
    let project_modules = local_modules.keys().cloned();
    dependency_modules
        .into_iter()
        .chain(project_modules)
        .sorted()
        .filter(|m| m.starts_with(&module.join("/")))
        .map(|label| CompletionItem {
            label,
            kind: Some(CompletionItemKind::MODULE),
            documentation: None,
            ..Default::default()
        })
        .collect()
}

/// Generate completions for expressions: in-scope values + module names for qualified access
fn completion_for_expression(
    type_info: &TypeInfo,
    compiler: &crate::server::lsp_project::LspProject,
) -> Option<Vec<CompletionItem>> {
    let mut items = Vec::new();

    // In-scope values: functions, constants, local variables
    for (name, constructor) in &type_info.values {
        // Skip local variables (they clutter autocomplete — user already knows them)
        if constructor.variant.is_local_variable() {
            continue;
        }

        // Skip ordinal-named arguments like "1st_arg", "2nd_arg" etc.
        if is_ordinal_argument_name(name) {
            continue;
        }

        items.push(value_to_completion_item(name, constructor));
    }

    // Add module names for qualified access (e.g. `module.`)
    for module_name in compiler.project.importable_modules() {
        // Don't suggest the current module itself
        if module_name == type_info.name {
            continue;
        }
        items.push(CompletionItem {
            label: module_name,
            kind: Some(CompletionItemKind::MODULE),
            documentation: None,
            ..Default::default()
        });
    }

    // Sort: in-scope values first (sorted by label), then modules
    // But we'll just sort everything by label for simplicity
    items.sort_by(|a, b| a.label.cmp(&b.label));

    Some(items)
}

/// Generate completions for patterns: find constructors matching the pattern type
fn completion_for_pattern(
    type_info: &TypeInfo,
    tipo: &std::rc::Rc<aiken_lang::tipo::Type>,
) -> Option<Vec<CompletionItem>> {
    let mut items = Vec::new();

    // Get the type name from the type to look up its constructors
    // Use Printer to get a readable type name
    let type_name = Printer::new().pretty_print(tipo.as_ref(), 0);

    // Look up constructors for this type name
    if let Some(constructor_names) = type_info.types_constructors.get(&type_name) {
        for name in constructor_names {
            if let Some(constructor) = type_info.values.get(name) {
                items.push(value_to_completion_item(name, constructor));
            }
        }
    }

    // If no constructors found by name, also try to find any constructor whose
    // return type matches. Fall back to listing all Record variants in scope.
    if items.is_empty() {
        for (name, constructor) in &type_info.values {
            if matches!(constructor.variant, ValueConstructorVariant::Record { .. }) {
                items.push(value_to_completion_item(name, constructor));
            }
        }
    }

    Some(items)
}

/// Generate completions for type annotations: list all types in scope
fn completion_for_annotation(type_info: &TypeInfo) -> Option<Vec<CompletionItem>> {
    let mut items = Vec::new();

    for (name, type_constructor) in &type_info.types {
        let type_str = Printer::new().pretty_print(&type_constructor.tipo, 0);

        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            detail: None,
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```aiken\n{}\n```", type_str),
            })),
            ..Default::default()
        });
    }

    items.sort_by(|a, b| a.label.cmp(&b.label));
    Some(items)
}

/// Generate completions for arguments: suggest labeled argument names from function field maps
fn completion_for_argument(type_info: &TypeInfo) -> Option<Vec<CompletionItem>> {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Collect labeled argument suggestions from all ModuleFn values in scope
    for constructor in type_info.values.values() {
        if let ValueConstructorVariant::ModuleFn {
            field_map: Some(ref field_map),
            ..
        } = constructor.variant
        {
            for (label, (_index, _span)) in &field_map.fields {
                if seen.insert(label.clone()) {
                    items.push(CompletionItem {
                        label: format!("{label}:"),
                        kind: Some(CompletionItemKind::PROPERTY),
                        detail: Some(format!("labeled argument `{label}`")),
                        documentation: None,
                        insert_text: Some(format!("{label}: ${{1}}")),
                        insert_text_format: Some(InsertTextFormat::SNIPPET),
                        ..Default::default()
                    });
                }
            }
        }
    }

    // Also suggest labeled fields from Record constructors
    for constructor in type_info.values.values() {
        if let ValueConstructorVariant::Record {
            field_map: Some(ref field_map),
            ..
        } = constructor.variant
        {
            for (label, (_index, _span)) in &field_map.fields {
                if seen.insert(label.clone()) {
                    items.push(CompletionItem {
                        label: format!("{label}:"),
                        kind: Some(CompletionItemKind::PROPERTY),
                        detail: Some(format!("record field `{label}`")),
                        documentation: None,
                        insert_text: Some(format!("{label}: ${{1}}")),
                        insert_text_format: Some(InsertTextFormat::SNIPPET),
                        ..Default::default()
                    });
                }
            }
        }
    }

    items.sort_by(|a, b| a.label.cmp(&b.label));
    Some(items)
}

/// Convert a ValueConstructor to an LSP CompletionItem
fn value_to_completion_item(name: &str, constructor: &ValueConstructor) -> CompletionItem {
    let (kind, detail) = match &constructor.variant {
        ValueConstructorVariant::LocalVariable { .. } => (CompletionItemKind::VARIABLE, None),
        ValueConstructorVariant::ModuleFn {
            arity,
            field_map,
            name: fn_name,
            ..
        } => {
            let sig = format_function_signature(name, *arity, field_map.as_ref());
            let detail = if fn_name != name {
                Some(format!("fn {sig}"))
            } else {
                Some(sig)
            };
            (CompletionItemKind::FUNCTION, detail)
        }
        ValueConstructorVariant::ModuleConstant {
            name: const_name, ..
        } => {
            let detail = if const_name != name {
                Some(format!("const {name}"))
            } else {
                Some("const".to_string())
            };
            (CompletionItemKind::CONSTANT, detail)
        }
        ValueConstructorVariant::Record {
            arity,
            field_map,
            constructors_count,
            ..
        } => {
            let detail_text = if *constructors_count > 1 {
                format!(
                    "{} (variant of {constructors_count})",
                    format_constructor_signature(name, *arity, field_map.as_ref())
                )
            } else {
                format_constructor_signature(name, *arity, field_map.as_ref())
            };
            (CompletionItemKind::CONSTRUCTOR, Some(detail_text))
        }
    };

    let type_str = Printer::new().pretty_print(&constructor.tipo, 0);

    let insert_text = format_insert_text(name, &constructor.variant);

    CompletionItem {
        label: name.to_string(),
        kind: Some(kind),
        detail,
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```aiken\n{}\n```", type_str),
        })),
        insert_text: Some(insert_text),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

/// Format a function signature string for display
fn format_function_signature(
    name: &str,
    arity: usize,
    field_map: Option<&aiken_lang::tipo::fields::FieldMap>,
) -> String {
    let args = match field_map {
        Some(fm) if !fm.fields.is_empty() => format_labeled_args(arity, fm),
        _ => (0..arity).map(|_| "_".to_string()).join(", "),
    };

    format!("{name}({args})")
}

fn format_labeled_args(arity: usize, field_map: &aiken_lang::tipo::fields::FieldMap) -> String {
    let mut args: Vec<String> = Vec::with_capacity(arity);
    let mut entries: Vec<(&String, &(usize, aiken_lang::ast::Span))> =
        field_map.fields.iter().collect();
    entries.sort_by_key(|(_, (idx, _))| *idx);

    let mut filled = 0;
    for (label, (idx, _)) in &entries {
        while filled < *idx {
            args.push("_".to_string());
            filled += 1;
        }
        args.push(label.to_string());
        filled += 1;
    }
    while filled < arity {
        args.push("_".to_string());
        filled += 1;
    }
    args.join(", ")
}

/// Format a constructor signature string for display
fn format_constructor_signature(
    name: &str,
    arity: usize,
    field_map: Option<&aiken_lang::tipo::fields::FieldMap>,
) -> String {
    if arity == 0 {
        return name.to_string();
    }

    let args = match field_map {
        Some(fm) if !fm.fields.is_empty() => format_labeled_args(arity, fm),
        _ => (0..arity).map(|_| "_".to_string()).join(", "),
    };

    format!("{name}({args})")
}

/// Generate snippet-based insert text for functions and constructors with arguments
fn format_insert_text(name: &str, variant: &ValueConstructorVariant) -> String {
    match variant {
        ValueConstructorVariant::ModuleFn {
            arity,
            field_map: Some(field_map),
            ..
        } if *arity > 0 && !field_map.fields.is_empty() => {
            let mut positional = field_map.fields.iter().collect::<Vec<_>>();
            positional.sort_by_key(|(_, (idx, _))| *idx);

            let mut parts = Vec::with_capacity(*arity);
            let mut filled = 0;
            for (label, (idx, _)) in &positional {
                while filled < *idx {
                    parts.push(format!("${{{}:_}}", filled + 1));
                    filled += 1;
                }
                parts.push(format!("{label}: ${{{}:_}}", filled + 1));
                filled += 1;
            }
            while filled < *arity {
                parts.push(format!("${{{}:_}}", filled + 1));
                filled += 1;
            }
            format!("{}({})", name, parts.join(", "))
        }
        ValueConstructorVariant::ModuleFn { arity, .. } if *arity > 0 => {
            let parts: Vec<String> = (1..=*arity).map(|i| format!("${{{i}:_}}")).collect();
            format!("{}({})", name, parts.join(", "))
        }
        ValueConstructorVariant::Record {
            arity,
            field_map: Some(field_map),
            ..
        } if *arity > 0 && !field_map.fields.is_empty() => {
            let mut positional = field_map.fields.iter().collect::<Vec<_>>();
            positional.sort_by_key(|(_, (idx, _))| *idx);

            let mut parts = Vec::with_capacity(*arity);
            let mut filled = 0;
            for (label, (idx, _)) in &positional {
                while filled < *idx {
                    parts.push(format!("${{{}:_}}", filled + 1));
                    filled += 1;
                }
                parts.push(format!("{label}: ${{{}:_}}", filled + 1));
                filled += 1;
            }
            while filled < *arity {
                parts.push(format!("${{{}:_}}", filled + 1));
                filled += 1;
            }
            format!("{}({})", name, parts.join(", "))
        }
        ValueConstructorVariant::Record { arity, .. } if *arity > 0 => {
            let parts: Vec<String> = (1..=*arity).map(|i| format!("${{{i}:_}}")).collect();
            format!("{}({})", name, parts.join(", "))
        }
        _ => name.to_string(),
    }
}

/// Returns true if the name looks like an auto-generated ordinal argument name (e.g. "1st_arg")
fn is_ordinal_argument_name(name: &str) -> bool {
    // Pattern: starts with digit, contains "st_arg" or "nd_arg" or "rd_arg" or "th_arg"
    if let Some(first_char) = name.chars().next()
        && first_char.is_ascii_digit()
    {
        return name.ends_with("_arg");
    }
    false
}
