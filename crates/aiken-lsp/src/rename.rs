use crate::{server::Server, utils::span_to_lsp_range};
use aiken_lang::{
    ast::{Definition, Located, Span, TypedDefinition},
    line_numbers::LineNumbers,
};
use lsp_types::{PrepareRenameResponse, RenameParams, TextEdit, WorkspaceEdit};
use std::collections::HashMap;

// ── Free helper functions (self-contained, no Server dependency) ──────────

/// Search source text within a span for a name string and return the narrowed Span.
fn find_identifier_span(source: &str, span: Span, name: &str) -> Option<Span> {
    let text = &source[span.start..span.end];
    let offset = text.find(name)?;
    Some(Span {
        start: span.start + offset,
        end: span.start + offset + name.len(),
    })
}

/// Extract the name string from a typed definition.
fn def_name(def: &TypedDefinition) -> Option<&str> {
    match def {
        Definition::Fn(f) => Some(&f.name),
        Definition::DataType(dt) => Some(&dt.name),
        Definition::TypeAlias(ta) => Some(&ta.alias),
        Definition::ModuleConstant(c) => Some(&c.name),
        Definition::Validator(v) => Some(&v.name),
        Definition::Test(t) => Some(&t.name),
        Definition::Benchmark(b) => Some(&b.name),
        Definition::Use(_) => None,
    }
}

// ── Server impl ───────────────────────────────────────────────────────────

impl Server {
    /// Handle `textDocument/prepareRename` — return the range of the identifier
    /// at the cursor position if renaming is supported for it.
    pub fn prepare_rename(
        &self,
        params: lsp_types::TextDocumentPositionParams,
    ) -> Option<PrepareRenameResponse> {
        let compiler = self.compiler.as_ref()?;
        let module = self.module_for_uri(&params.text_document.uri)?;
        let line_numbers = LineNumbers::new(&module.code);

        let target = self.find_symbol_at_position(&params.position, module, &line_numbers)?;

        // Block renaming of dependency symbols
        let own_package = compiler.own_package();
        if let Some(def_module) = &target.def_module
            && let Some(def_mod) = compiler.modules.get(def_module)
            && def_mod.package != *own_package
        {
            return None;
        }

        let byte_index = line_numbers.byte_index(
            params.position.line as usize,
            params.position.character as usize,
        );

        if let Some(node) = module.find_node(byte_index)
            && let Some(name_span) = identifier_span_for_node(&node, &module.code)
        {
            let range = span_to_lsp_range(name_span, &line_numbers);
            return Some(PrepareRenameResponse::Range(range));
        }

        None
    }

    /// Handle `textDocument/rename` — rename a symbol and all its references.
    pub fn rename(&self, params: RenameParams) -> Option<WorkspaceEdit> {
        let compiler = self.compiler.as_ref()?;
        let module = self.module_for_uri(&params.text_document_position.text_document.uri)?;
        let line_numbers = LineNumbers::new(&module.code);

        let target = self.find_symbol_at_position(
            &params.text_document_position.position,
            module,
            &line_numbers,
        )?;

        // Block renaming of dependency symbols
        let own_package = compiler.own_package();
        if let Some(def_module) = &target.def_module
            && let Some(def_mod) = compiler.modules.get(def_module)
            && def_mod.package != *own_package
        {
            return None;
        }

        let mut changes: HashMap<lsp_types::Url, Vec<TextEdit>> = HashMap::new();
        let new_name = &params.new_name;

        // Collect references from all modules
        for (module_name, checked_module) in &compiler.modules {
            if let Some(locations) =
                self.find_references_in_module(&target, checked_module, module_name)
            {
                for location in locations {
                    changes.entry(location.uri).or_default().push(TextEdit {
                        range: location.range,
                        new_text: new_name.clone(),
                    });
                }
            }
        }

        // Include the definition site (identifier span only)
        let def_mod_name = target.def_module.as_deref().unwrap_or(&module.name);
        if let Some(def_mod) = compiler.modules.get(def_mod_name)
            && let Some(uri) = self.module_name_to_uri(def_mod_name)
            && let Some(name_span) =
                find_identifier_span(&def_mod.code, target.def_span, &target.name)
        {
            let def_ln = LineNumbers::new(&def_mod.code);
            let range = span_to_lsp_range(name_span, &def_ln);
            changes.entry(uri).or_default().push(TextEdit {
                range,
                new_text: new_name.clone(),
            });
        }

        if changes.is_empty() {
            return None;
        }

        Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        })
    }
}

// ── Local helpers ──────────────────────────────────────────────────────────

/// Extract the span of the identifier name within a `Located` AST node.
fn identifier_span_for_node(node: &Located<'_>, source: &str) -> Option<Span> {
    match node {
        Located::Definition(def) => {
            let name = def_name(def)?;
            find_identifier_span(source, def.location(), name)
        }
        Located::Expression(aiken_lang::expr::TypedExpr::Var { name, location, .. }) => {
            find_identifier_span(source, *location, name)
        }
        _ => None,
    }
}
