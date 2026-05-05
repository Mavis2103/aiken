use aiken_lang::{
    ast::{Definition, Span, TypedDefinition, TypedPattern},
    expr::TypedExpr,
    line_numbers::LineNumbers,
    parser::extra::ModuleExtra,
};
use aiken_project::module::CheckedModule;
use lsp_types::{SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend};

const FUNCTION: u32 = 0;
const VARIABLE: u32 = 1;
const CLASS: u32 = 2;
const CONSTRUCTOR: u32 = 3;
const KEYWORD: u32 = 4;
const STRING: u32 = 5;
const NUMBER: u32 = 6;
const COMMENT: u32 = 7;
const NAMESPACE: u32 = 8;
const TYPE: u32 = 9;
const OPERATOR: u32 = 10;
const ENUM_MEMBER: u32 = 11;

const DECLARATION_BIT: u32 = 1 << 0;
const DOCUMENTATION_BIT: u32 = 1 << 1;
const READONLY_BIT: u32 = 1 << 2;

pub fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::FUNCTION,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::CLASS,
            SemanticTokenType::new("constructor"),
            SemanticTokenType::KEYWORD,
            SemanticTokenType::STRING,
            SemanticTokenType::NUMBER,
            SemanticTokenType::COMMENT,
            SemanticTokenType::NAMESPACE,
            SemanticTokenType::TYPE,
            SemanticTokenType::OPERATOR,
            SemanticTokenType::ENUM_MEMBER,
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::DOCUMENTATION,
            SemanticTokenModifier::READONLY,
        ],
    }
}

pub fn semantic_tokens_full(module: &CheckedModule) -> Vec<u32> {
    let line_numbers = LineNumbers::new(&module.code);
    let mut collector = RawCollector::new(&line_numbers);

    collect_doc_comments(&mut collector, &module.extra, &module.code);

    for def in &module.ast.definitions {
        collector.collect_definition(def);
    }

    collect_keywords(&mut collector, &module.code, &line_numbers);

    collector
        .tokens
        .sort_by(|a, b| a.line.cmp(&b.line).then(a.start_char.cmp(&b.start_char)));

    collector.tokens.retain(|t| t.length > 0);

    encode(collector.tokens)
}

struct RawToken {
    line: u32,
    start_char: u32,
    length: u32,
    token_type: u32,
    token_modifiers: u32,
}

fn encode(tokens: Vec<RawToken>) -> Vec<u32> {
    let mut out = Vec::with_capacity(tokens.len() * 5);
    let mut prev_line: u32 = 0;
    let mut prev_start: u32 = 0;

    for t in &tokens {
        let delta_line = t.line.wrapping_sub(prev_line);
        let delta_start = if delta_line == 0 {
            t.start_char.wrapping_sub(prev_start)
        } else {
            t.start_char
        };

        out.push(delta_line);
        out.push(delta_start);
        out.push(t.length);
        out.push(t.token_type);
        out.push(t.token_modifiers);

        prev_line = t.line;
        prev_start = t.start_char;
    }

    out
}

struct RawCollector<'a> {
    line_numbers: &'a LineNumbers,
    tokens: Vec<RawToken>,
}

impl<'a> RawCollector<'a> {
    fn new(line_numbers: &'a LineNumbers) -> Self {
        Self {
            line_numbers,
            tokens: Vec::new(),
        }
    }

    fn push(&mut self, span: Span, token_type: u32, token_modifiers: u32) {
        let length = span.end.saturating_sub(span.start) as u32;
        if length == 0 {
            return;
        }
        let start = self
            .line_numbers
            .line_and_column_number(span.start)
            .unwrap();
        self.tokens.push(RawToken {
            line: (start.line - 1) as u32,
            start_char: (start.column - 1) as u32,
            length,
            token_type,
            token_modifiers,
        });
    }

    fn collect_definition(&mut self, def: &TypedDefinition) {
        match def {
            Definition::Fn(func) => {
                self.push(func.location, FUNCTION, DECLARATION_BIT);
                for arg in &func.arguments {
                    self.collect_arg(arg);
                }
                self.collect_expr(&func.body);
            }
            Definition::Test(func) | Definition::Benchmark(func) => {
                self.push(func.location, FUNCTION, DECLARATION_BIT);
                for arg_via in &func.arguments {
                    self.collect_arg(&arg_via.arg);
                }
                self.collect_expr(&func.body);
            }
            Definition::TypeAlias(ta) => {
                self.push(ta.location, CLASS, DECLARATION_BIT);
                self.collect_annotation(&ta.annotation);
            }
            Definition::DataType(dt) => {
                self.push(dt.location, CLASS, DECLARATION_BIT);
                for ctor in &dt.constructors {
                    self.push(ctor.location, CONSTRUCTOR, DECLARATION_BIT);
                    for field in &ctor.arguments {
                        self.collect_annotation(&field.annotation);
                    }
                }
            }
            Definition::ModuleConstant(mc) => {
                self.push(mc.location, VARIABLE, READONLY_BIT);
                if let Some(ann) = &mc.annotation {
                    self.collect_annotation(ann);
                }
                self.collect_expr(&mc.value);
            }
            Definition::Validator(v) => {
                self.push(v.location, CLASS, DECLARATION_BIT);
                for param in &v.params {
                    self.collect_arg(param);
                }
                for handler in &v.handlers {
                    self.push(handler.location, FUNCTION, DECLARATION_BIT);
                    for arg in &handler.arguments {
                        self.collect_arg(arg);
                    }
                    self.collect_expr(&handler.body);
                }
                for arg in &v.fallback.arguments {
                    self.collect_arg(arg);
                }
                self.collect_expr(&v.fallback.body);
            }
            Definition::Use(u) => {
                self.push(u.location, NAMESPACE, 0);
                for imp in &u.unqualified.1 {
                    self.push(imp.location, FUNCTION, 0);
                }
            }
        }
    }

    fn collect_arg(&mut self, arg: &aiken_lang::ast::TypedArg) {
        self.push(arg.arg_name.location(), VARIABLE, DECLARATION_BIT);
        if let Some(ann) = &arg.annotation {
            self.collect_annotation(ann);
        }
    }

    fn collect_expr(&mut self, expr: &TypedExpr) {
        match expr {
            TypedExpr::UInt { location, .. } | TypedExpr::String { location, .. } => {
                let ty = if matches!(expr, TypedExpr::String { .. }) {
                    STRING
                } else {
                    NUMBER
                };
                self.push(*location, ty, 0);
            }
            TypedExpr::ByteArray { location, .. } => {
                self.push(*location, STRING, 0);
            }
            TypedExpr::CurvePoint { location, .. } => {
                self.push(*location, STRING, 0);
            }
            TypedExpr::Sequence { expressions, .. } => {
                for e in expressions {
                    self.collect_expr(e);
                }
            }
            TypedExpr::Pipeline { expressions, .. } => {
                for e in expressions.iter() {
                    self.collect_expr(e);
                }
            }
            TypedExpr::Var { location, .. } => {
                self.push(*location, FUNCTION, 0);
            }
            TypedExpr::Fn { args, body, .. } => {
                for arg in args {
                    self.collect_arg(arg);
                }
                self.collect_expr(body);
            }
            TypedExpr::List { elements, tail, .. } => {
                for e in elements {
                    self.collect_expr(e);
                }
                if let Some(t) = tail {
                    self.collect_expr(t);
                }
            }
            TypedExpr::Call { fun, args, .. } => {
                self.collect_expr(fun);
                for arg in args {
                    self.collect_expr(&arg.value);
                }
            }
            TypedExpr::BinOp {
                location,
                left,
                right,
                ..
            } => {
                self.push(*location, OPERATOR, 0);
                self.collect_expr(left);
                self.collect_expr(right);
            }
            TypedExpr::Assignment { value, pattern, .. } => {
                self.collect_pattern(pattern, DECLARATION_BIT);
                self.collect_expr(value);
            }
            TypedExpr::Trace { then, text, .. } => {
                self.collect_expr(then);
                self.collect_expr(text);
            }
            TypedExpr::When {
                subject, clauses, ..
            } => {
                self.collect_expr(subject);
                for clause in clauses {
                    self.collect_pattern(&clause.pattern, 0);
                    self.collect_expr(&clause.then);
                }
            }
            TypedExpr::If {
                branches,
                final_else,
                ..
            } => {
                for branch in branches.iter() {
                    if let Some((pat, _)) = &branch.is {
                        self.collect_pattern(pat, DECLARATION_BIT);
                    }
                    self.collect_expr(&branch.condition);
                    self.collect_expr(&branch.body);
                }
                self.collect_expr(final_else);
            }
            TypedExpr::RecordAccess { record, .. } => {
                self.collect_expr(record);
            }
            TypedExpr::ModuleSelect { location, .. } => {
                self.push(*location, CONSTRUCTOR, 0);
            }
            TypedExpr::Tuple { elems, .. } => {
                for e in elems {
                    self.collect_expr(e);
                }
            }
            TypedExpr::Pair { fst, snd, .. } => {
                self.collect_expr(fst);
                self.collect_expr(snd);
            }
            TypedExpr::TupleIndex { tuple, .. } => {
                self.collect_expr(tuple);
            }
            TypedExpr::RecordUpdate { spread, args, .. } => {
                self.collect_expr(spread);
                for arg in args {
                    self.collect_expr(&arg.value);
                }
            }
            TypedExpr::UnOp {
                location, value, ..
            } => {
                self.push(*location, OPERATOR, 0);
                self.collect_expr(value);
            }
            TypedExpr::ErrorTerm { .. } => {}
        }
    }

    fn collect_pattern(&mut self, pattern: &TypedPattern, extra_modifiers: u32) {
        match pattern {
            TypedPattern::Var { location, .. } => {
                self.push(*location, VARIABLE, extra_modifiers);
            }
            TypedPattern::Assign {
                location, pattern, ..
            } => {
                self.push(*location, VARIABLE, extra_modifiers);
                self.collect_pattern(pattern, extra_modifiers);
            }
            TypedPattern::Int { location, .. } => {
                self.push(*location, NUMBER, 0);
            }
            TypedPattern::ByteArray { location, .. } => {
                self.push(*location, STRING, 0);
            }
            TypedPattern::List { elements, tail, .. } => {
                for e in elements {
                    self.collect_pattern(e, extra_modifiers);
                }
                if let Some(t) = tail {
                    self.collect_pattern(t, extra_modifiers);
                }
            }
            TypedPattern::Pair { fst, snd, .. } => {
                self.collect_pattern(fst, extra_modifiers);
                self.collect_pattern(snd, extra_modifiers);
            }
            TypedPattern::Tuple { elems, .. } => {
                for e in elems {
                    self.collect_pattern(e, extra_modifiers);
                }
            }
            TypedPattern::Constructor {
                location,
                arguments,
                ..
            } => {
                self.push(*location, ENUM_MEMBER, 0);
                for arg in arguments {
                    self.collect_pattern(&arg.value, extra_modifiers);
                }
            }
            TypedPattern::Discard { .. } => {}
        }
    }

    fn collect_annotation(&mut self, ann: &aiken_lang::ast::Annotation) {
        match ann {
            aiken_lang::ast::Annotation::Constructor {
                location,
                arguments,
                ..
            } => {
                self.push(*location, TYPE, 0);
                for arg in arguments {
                    self.collect_annotation(arg);
                }
            }
            aiken_lang::ast::Annotation::Fn { arguments, ret, .. } => {
                for arg in arguments {
                    self.collect_annotation(arg);
                }
                self.collect_annotation(ret);
            }
            aiken_lang::ast::Annotation::Var { location, .. } => {
                self.push(*location, TYPE, 0);
            }
            aiken_lang::ast::Annotation::Tuple { elems, .. } => {
                for e in elems {
                    self.collect_annotation(e);
                }
            }
            aiken_lang::ast::Annotation::Pair { fst, snd, .. } => {
                self.collect_annotation(fst);
                self.collect_annotation(snd);
            }
            aiken_lang::ast::Annotation::Hole { .. } => {}
        }
    }
}

fn collect_doc_comments(collector: &mut RawCollector<'_>, extra: &ModuleExtra, source: &str) {
    for span in &extra.doc_comments {
        if span.end > span.start {
            collector.push(*span, COMMENT, DOCUMENTATION_BIT);
        }
    }
    for span in &extra.comments {
        if span.end > span.start {
            collector.push(*span, COMMENT, 0);
        }
    }
    let _ = source;
}

const KEYWORDS: &[&str] = &[
    "as",
    "const",
    "else",
    "expect",
    "fail",
    "fn",
    "if",
    "is",
    "let",
    "opaque",
    "pub",
    "test",
    "todo",
    "trace",
    "type",
    "use",
    "validator",
    "via",
    "when",
    "and",
    "or",
    "bench",
];

fn collect_keywords(collector: &mut RawCollector<'_>, source: &str, line_numbers: &LineNumbers) {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        while i < len && !is_ident_start(bytes[i]) {
            i += 1;
        }
        if i >= len {
            break;
        }

        let start = i;
        while i < len && is_ident_continue(bytes[i]) {
            i += 1;
        }

        let word = &source[start..i];
        let word_lower = word.to_lowercase();

        if KEYWORDS.contains(&word_lower.as_str()) {
            let span = Span { start, end: i };
            collector.push(span, KEYWORD, 0);
        }
    }

    let _ = line_numbers;
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
