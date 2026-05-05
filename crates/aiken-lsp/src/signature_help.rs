use crate::server::Server;
use aiken_lang::{
    ast::{CallArg, Definition, Span, TypedDefinition, TypedModule, TypedPattern},
    expr::TypedExpr,
    line_numbers::LineNumbers,
    tipo::{
        ModuleValueConstructor, Type, ValueConstructor, ValueConstructorVariant, fields::FieldMap,
        pretty::Printer,
    },
};
use lsp_types::{
    ParameterInformation, ParameterLabel, SignatureHelp, SignatureHelpParams, SignatureInformation,
};
use std::rc::Rc;

pub fn signature_help(server: &Server, params: SignatureHelpParams) -> Option<SignatureHelp> {
    let module = server.module_for_uri(&params.text_document_position_params.text_document.uri)?;
    let line_numbers = LineNumbers::new(&module.code);
    let byte_index = line_numbers.byte_index(
        params.text_document_position_params.position.line as usize,
        params.text_document_position_params.position.character as usize,
    );

    let (constructor, active_param) = find_enclosing_call_at(&module.ast, byte_index)?;

    let signature = build_signature(&constructor, active_param)?;

    Some(SignatureHelp {
        signatures: vec![signature],
        active_signature: Some(0),
        active_parameter: Some(active_param as u32),
    })
}

/// Walk module definitions for the innermost Call node whose span contains `byte_index`,
/// returning the called function's `ValueConstructor` and the active parameter index.
fn find_enclosing_call_at(
    ast: &TypedModule,
    byte_index: usize,
) -> Option<(ValueConstructor, usize)> {
    ast.definitions
        .iter()
        .find_map(|def| find_call_in_definition(def, byte_index))
}

fn find_call_in_definition(
    definition: &TypedDefinition,
    byte_index: usize,
) -> Option<(ValueConstructor, usize)> {
    match definition {
        Definition::Fn(func) => find_call_in_expr(&func.body, byte_index),
        Definition::Test(test) => find_call_in_expr(&test.body, byte_index),
        Definition::Benchmark(bench) => find_call_in_expr(&bench.body, byte_index),
        Definition::Validator(validator) => validator
            .handlers
            .iter()
            .find_map(|handler| find_call_in_expr(&handler.body, byte_index))
            .or_else(|| find_call_in_expr(&validator.fallback.body, byte_index)),
        Definition::ModuleConstant(constant) => find_call_in_expr(&constant.value, byte_index),
        _ => None,
    }
}

fn find_call_in_expr(expr: &TypedExpr, byte_index: usize) -> Option<(ValueConstructor, usize)> {
    if !expr.location().contains(byte_index) {
        return None;
    }

    match expr {
        TypedExpr::Call {
            fun,
            args,
            location,
            ..
        } => {
            for arg in args {
                if let Some(result) = find_call_in_expr(&arg.value, byte_index) {
                    return Some(result);
                }
            }
            if let Some(result) = find_call_in_expr(fun, byte_index) {
                return Some(result);
            }

            let constructor = match fun.as_ref() {
                TypedExpr::Var { constructor, .. } => constructor.clone(),
                TypedExpr::ModuleSelect {
                    constructor, tipo, ..
                } => match constructor {
                    ModuleValueConstructor::Record {
                        name,
                        arity,
                        tipo,
                        field_map,
                        location,
                    } => ValueConstructor {
                        public: true,
                        variant: ValueConstructorVariant::Record {
                            name: name.clone(),
                            arity: *arity,
                            field_map: field_map.clone(),
                            location: *location,
                            module: String::new(),
                            constructors_count: 1,
                        },
                        tipo: tipo.clone(),
                    },
                    ModuleValueConstructor::Fn {
                        module,
                        name,
                        location,
                    } => ValueConstructor {
                        public: true,
                        variant: ValueConstructorVariant::ModuleFn {
                            name: name.clone(),
                            field_map: None,
                            module: module.clone(),
                            arity: 0,
                            location: *location,
                            builtin: None,
                        },
                        tipo: tipo.clone(),
                    },
                    _ => return None,
                },
                _ => return None,
            };

            let active_param = count_args_before_cursor(args, byte_index, location);

            Some((constructor, active_param))
        }

        TypedExpr::Sequence { expressions, .. } | TypedExpr::Pipeline { expressions, .. } => {
            expressions
                .iter()
                .find_map(|e| find_call_in_expr(e, byte_index))
        }

        TypedExpr::Fn { body, .. } => find_call_in_expr(body, byte_index),

        TypedExpr::Assignment { value, pattern, .. } => {
            find_call_in_expr_in_pattern(pattern, byte_index)
                .or_else(|| find_call_in_expr(value, byte_index))
        }

        TypedExpr::List { elements, tail, .. } => elements
            .iter()
            .find_map(|e| find_call_in_expr(e, byte_index))
            .or_else(|| tail.as_ref().and_then(|t| find_call_in_expr(t, byte_index))),

        TypedExpr::BinOp { left, right, .. } => {
            find_call_in_expr(left, byte_index).or_else(|| find_call_in_expr(right, byte_index))
        }

        TypedExpr::Trace { then, text, .. } => {
            find_call_in_expr(then, byte_index).or_else(|| find_call_in_expr(text, byte_index))
        }

        TypedExpr::When {
            subject, clauses, ..
        } => find_call_in_expr(subject, byte_index).or_else(|| {
            clauses.iter().find_map(|clause| {
                find_call_in_expr_in_pattern(&clause.pattern, byte_index)
                    .or_else(|| find_call_in_expr(&clause.then, byte_index))
            })
        }),

        TypedExpr::If {
            branches,
            final_else,
            ..
        } => branches
            .iter()
            .find_map(|branch| {
                find_call_in_expr(&branch.condition, byte_index)
                    .or_else(|| find_call_in_expr(&branch.body, byte_index))
            })
            .or_else(|| find_call_in_expr(final_else, byte_index)),

        TypedExpr::RecordAccess { record, .. } | TypedExpr::TupleIndex { tuple: record, .. } => {
            find_call_in_expr(record, byte_index)
        }

        TypedExpr::RecordUpdate { spread, args, .. } => find_call_in_expr(spread, byte_index)
            .or_else(|| {
                args.iter()
                    .find_map(|arg| find_call_in_expr(&arg.value, byte_index))
            }),

        TypedExpr::Tuple { elems, .. } => {
            elems.iter().find_map(|e| find_call_in_expr(e, byte_index))
        }

        TypedExpr::Pair { fst, snd, .. } => {
            find_call_in_expr(fst, byte_index).or_else(|| find_call_in_expr(snd, byte_index))
        }

        TypedExpr::UnOp { value, .. } => find_call_in_expr(value, byte_index),

        TypedExpr::Var { .. }
        | TypedExpr::UInt { .. }
        | TypedExpr::String { .. }
        | TypedExpr::ByteArray { .. }
        | TypedExpr::ErrorTerm { .. }
        | TypedExpr::CurvePoint { .. }
        | TypedExpr::ModuleSelect { .. } => None,
    }
}

fn find_call_in_expr_in_pattern(
    _pattern: &TypedPattern,
    _byte_index: usize,
) -> Option<(ValueConstructor, usize)> {
    None
}

fn count_args_before_cursor(
    args: &[CallArg<TypedExpr>],
    byte_index: usize,
    _call_location: &Span,
) -> usize {
    let mut count = 0;
    for arg in args {
        if byte_index > arg.location.end {
            count += 1;
        } else {
            break;
        }
    }
    count
}

fn build_signature(
    constructor: &ValueConstructor,
    active_param: usize,
) -> Option<SignatureInformation> {
    match &constructor.variant {
        ValueConstructorVariant::ModuleFn {
            name,
            arity,
            field_map,
            ..
        }
        | ValueConstructorVariant::Record {
            name,
            arity,
            field_map,
            ..
        } => Some(build_fn_signature(
            name,
            *arity,
            field_map.as_ref(),
            &constructor.tipo,
            active_param,
        )),

        _ => None,
    }
}

fn format_return_type(tipo: &Rc<Type>) -> String {
    match tipo.as_ref() {
        Type::Fn { ret, .. } => {
            let mut printer = Printer::new();
            printer.pretty_print(ret, 0)
        }
        _ => {
            let mut printer = Printer::new();
            printer.pretty_print(tipo, 0)
        }
    }
}

fn build_fn_signature(
    name: &str,
    arity: usize,
    field_map: Option<&FieldMap>,
    tipo: &Rc<Type>,
    active_param: usize,
) -> SignatureInformation {
    let return_type = format_return_type(tipo);
    let parameters = build_parameters(field_map, arity);

    let param_string = if let Some(fm) = field_map {
        let mut sorted: Vec<(&String, &(usize, Span))> = fm.fields.iter().collect();
        sorted.sort_by_key(|(_, (idx, _))| *idx);
        sorted
            .iter()
            .map(|(label, _)| label.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    } else if arity > 0 {
        (0..arity)
            .map(|i| format!("arg{}", i))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        String::new()
    };

    let label = if arity == 0 {
        format!("type: {}", return_type)
    } else {
        format!("fn {}({})", name, param_string)
    };

    SignatureInformation {
        label,
        documentation: None,
        parameters: Some(parameters),
        active_parameter: Some(active_param as u32),
    }
}

fn build_parameters(field_map: Option<&FieldMap>, arity: usize) -> Vec<ParameterInformation> {
    if let Some(fm) = field_map {
        let mut sorted: Vec<(&String, &(usize, Span))> = fm.fields.iter().collect();
        sorted.sort_by_key(|(_, (idx, _))| *idx);

        sorted
            .iter()
            .map(|(label, (_, _))| ParameterInformation {
                label: ParameterLabel::Simple(label.to_string()),
                documentation: None,
            })
            .collect()
    } else if arity > 0 {
        (0..arity)
            .map(|i| ParameterInformation {
                label: ParameterLabel::Simple(format!("arg{}", i)),
                documentation: None,
            })
            .collect()
    } else {
        vec![]
    }
}
