use crate::server::Server;
use crate::utils::span_to_lsp_range;
use aiken_lang::ast::{ArgName, Definition, Span, TypedDefinition, TypedFunction};
use aiken_lang::expr::TypedExpr;
use aiken_lang::line_numbers::LineNumbers;
use aiken_lang::tipo::pretty::Printer;
use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams};

impl Server {
    /// Implements the `textDocument/inlayHints` request.
    /// Returns inline type annotations for unannotated let bindings,
    /// function arguments, and return types.
    pub fn inlay_hints(&self, params: InlayHintParams) -> Option<Vec<InlayHint>> {
        let module = self.module_for_uri(&params.text_document.uri)?;
        let line_numbers = LineNumbers::new(&module.code);

        let range_start = line_numbers.byte_index(
            params.range.start.line as usize,
            params.range.start.character as usize,
        );
        let range_end = line_numbers.byte_index(
            params.range.end.line as usize,
            params.range.end.character as usize,
        );

        let mut printer = Printer::new();
        let mut hints = Vec::new();

        for definition in &module.ast.definitions {
            let def_loc = definition.location();
            if def_loc.start > range_end || def_loc.end <= range_start {
                continue;
            }
            collect_definition_hints(definition, &line_numbers, &mut printer, &mut hints);
        }

        if hints.is_empty() { None } else { Some(hints) }
    }
}

fn collect_definition_hints(
    def: &TypedDefinition,
    line_numbers: &LineNumbers,
    printer: &mut Printer,
    hints: &mut Vec<InlayHint>,
) {
    match def {
        Definition::Fn(f) => collect_function_hints(f, line_numbers, printer, hints),
        Definition::Validator(v) => {
            for param in &v.params {
                collect_arg_hint(param, line_numbers, printer, hints);
            }
            for handler in &v.handlers {
                collect_function_hints(handler, line_numbers, printer, hints);
            }
            collect_function_hints(&v.fallback, line_numbers, printer, hints);
        }
        Definition::Test(f) => {
            for arg_via in &f.arguments {
                collect_arg_hint(&arg_via.arg, line_numbers, printer, hints);
            }
            if f.return_annotation.is_none() {
                let return_type_str = printer.pretty_print(&f.return_type, 0);
                let body_start = f.body.location().start;
                let pos = span_to_lsp_range(
                    Span {
                        start: body_start,
                        end: body_start,
                    },
                    line_numbers,
                )
                .start;
                hints.push(make_hint(pos, format!("-> {} ", return_type_str)));
            }
            collect_expr_hints(&f.body, line_numbers, printer, hints);
        }
        Definition::Benchmark(f) => {
            for arg_via in &f.arguments {
                collect_arg_hint(&arg_via.arg, line_numbers, printer, hints);
            }
            if f.return_annotation.is_none() {
                let return_type_str = printer.pretty_print(&f.return_type, 0);
                let body_start = f.body.location().start;
                let pos = span_to_lsp_range(
                    Span {
                        start: body_start,
                        end: body_start,
                    },
                    line_numbers,
                )
                .start;
                hints.push(make_hint(pos, format!("-> {} ", return_type_str)));
            }
            collect_expr_hints(&f.body, line_numbers, printer, hints);
        }
        _ => {}
    }
}

fn collect_function_hints(
    f: &TypedFunction,
    line_numbers: &LineNumbers,
    printer: &mut Printer,
    hints: &mut Vec<InlayHint>,
) {
    for arg in &f.arguments {
        collect_arg_hint(arg, line_numbers, printer, hints);
    }
    if f.return_annotation.is_none() {
        let return_type_str = printer.pretty_print(&f.return_type, 0);
        let body_start = f.body.location().start;
        let pos = span_to_lsp_range(
            Span {
                start: body_start,
                end: body_start,
            },
            line_numbers,
        )
        .start;
        hints.push(make_hint(pos, format!("-> {} ", return_type_str)));
    }
    collect_expr_hints(&f.body, line_numbers, printer, hints);
}

fn collect_arg_hint(
    arg: &aiken_lang::ast::TypedArg,
    line_numbers: &LineNumbers,
    printer: &mut Printer,
    hints: &mut Vec<InlayHint>,
) {
    // Skip discarded arguments (e.g. _foo)
    if matches!(arg.arg_name, ArgName::Discarded { .. }) {
        return;
    }
    if arg.annotation.is_some() {
        return;
    }
    let type_str = printer.pretty_print(&arg.tipo, 0);
    let pos = span_to_lsp_range(
        Span {
            start: arg.arg_name.location().end,
            end: arg.arg_name.location().end,
        },
        line_numbers,
    )
    .start;
    hints.push(make_hint(pos, format!(": {}", type_str)));
}

fn collect_expr_hints(
    expr: &TypedExpr,
    line_numbers: &LineNumbers,
    printer: &mut Printer,
    hints: &mut Vec<InlayHint>,
) {
    match expr {
        TypedExpr::Assignment {
            pattern,
            tipo,
            value,
            ..
        } => {
            let type_str = printer.pretty_print(tipo, 0);
            let pattern_end = pattern.location().end;
            let pos = span_to_lsp_range(
                Span {
                    start: pattern_end,
                    end: pattern_end,
                },
                line_numbers,
            )
            .start;
            hints.push(make_hint(pos, format!(": {}", type_str)));
            collect_expr_hints(value, line_numbers, printer, hints);
        }
        TypedExpr::Sequence { expressions, .. } | TypedExpr::Pipeline { expressions, .. } => {
            for e in expressions {
                collect_expr_hints(e, line_numbers, printer, hints);
            }
        }
        TypedExpr::Call { fun, args, .. } => {
            collect_expr_hints(fun, line_numbers, printer, hints);
            for arg in args {
                collect_expr_hints(&arg.value, line_numbers, printer, hints);
            }
        }
        TypedExpr::When {
            subject, clauses, ..
        } => {
            collect_expr_hints(subject, line_numbers, printer, hints);
            for clause in clauses {
                collect_expr_hints(&clause.then, line_numbers, printer, hints);
            }
        }
        TypedExpr::If {
            branches,
            final_else,
            ..
        } => {
            for branch in branches.iter() {
                collect_expr_hints(&branch.condition, line_numbers, printer, hints);
                collect_expr_hints(&branch.body, line_numbers, printer, hints);
            }
            collect_expr_hints(final_else, line_numbers, printer, hints);
        }
        TypedExpr::Fn {
            args,
            body,
            return_annotation,
            ..
        } => {
            for arg in args {
                collect_arg_hint(arg, line_numbers, printer, hints);
            }
            if return_annotation.is_none() {
                // For anonymous functions, we can show a return type hint before the body
                let body_start = body.location().start;
                let return_type_str = printer.pretty_print(&expr.tipo(), 0);
                let pos = span_to_lsp_range(
                    Span {
                        start: body_start,
                        end: body_start,
                    },
                    line_numbers,
                )
                .start;
                hints.push(make_hint(pos, format!("-> {} ", return_type_str)));
            }
            collect_expr_hints(body, line_numbers, printer, hints);
        }
        TypedExpr::Trace { then, text, .. } => {
            collect_expr_hints(then, line_numbers, printer, hints);
            collect_expr_hints(text, line_numbers, printer, hints);
        }
        TypedExpr::BinOp { left, right, .. } => {
            collect_expr_hints(left, line_numbers, printer, hints);
            collect_expr_hints(right, line_numbers, printer, hints);
        }
        TypedExpr::UnOp { value, .. } => {
            collect_expr_hints(value, line_numbers, printer, hints);
        }
        TypedExpr::List { elements, tail, .. } => {
            for e in elements {
                collect_expr_hints(e, line_numbers, printer, hints);
            }
            if let Some(t) = tail {
                collect_expr_hints(t, line_numbers, printer, hints);
            }
        }
        TypedExpr::RecordAccess { record, .. } => {
            collect_expr_hints(record, line_numbers, printer, hints);
        }
        TypedExpr::Tuple { elems, .. } => {
            for e in elems {
                collect_expr_hints(e, line_numbers, printer, hints);
            }
        }
        TypedExpr::Pair { fst, snd, .. } => {
            collect_expr_hints(fst, line_numbers, printer, hints);
            collect_expr_hints(snd, line_numbers, printer, hints);
        }
        TypedExpr::TupleIndex { tuple, .. } => {
            collect_expr_hints(tuple, line_numbers, printer, hints);
        }
        TypedExpr::RecordUpdate { spread, args, .. } => {
            collect_expr_hints(spread, line_numbers, printer, hints);
            for arg in args {
                collect_expr_hints(&arg.value, line_numbers, printer, hints);
            }
        }
        // Leaves: no sub-expressions to recurse into
        TypedExpr::UInt { .. }
        | TypedExpr::String { .. }
        | TypedExpr::ByteArray { .. }
        | TypedExpr::CurvePoint { .. }
        | TypedExpr::Var { .. }
        | TypedExpr::ModuleSelect { .. }
        | TypedExpr::ErrorTerm { .. } => {}
    }
}

fn make_hint(position: lsp_types::Position, label: String) -> InlayHint {
    InlayHint {
        position,
        label: InlayHintLabel::String(label),
        kind: Some(InlayHintKind::TYPE),
        tooltip: None,
        padding_left: Some(true),
        padding_right: Some(false),
        data: None,
        text_edits: None,
    }
}
