//! Calls and subsets: `f(...)`, `x[...]`, `x[[...]]`
//!
//! Shape: `R_CALL { function, R_CALL_ARGUMENTS { '(', R_ARGUMENT_LIST, ')' } }`
//! (same for `R_SUBSET` with `[ ]` and `R_SUBSET2` with `[[ ]]`).
//!
//! Two shapes are synthesized relative to tree-sitter:
//!
//! - Argument *holes* (`x[, 1]`, `fn(, )`, `fn(a, )`) become explicit empty
//!   `R_ARGUMENT` nodes. Mirroring the tree-sitter walker: a hole is emitted
//!   before a comma when the previous element was a comma or the open
//!   delimiter, and before the close when the previous element was a comma.
//!   Comments are trivia here, so they are transparent automatically.
//! - Named arguments get `R_ARGUMENT_NAME_CLAUSE { name, '=' }`, where the
//!   name is an identifier, string, `...`, `..i`, or `NULL`.

use air_r_syntax::RSyntaxKind;
use air_r_syntax::RSyntaxKind::*;
use biome_parser::Parser;
use biome_parser::prelude::CompletedMarker;

use crate::grammar::atoms;
use crate::grammar::expected;
use crate::grammar::expressions::parse_expression;
use crate::grammar::expressions::parse_expression_rest;
use crate::parser::RParser;

pub(crate) fn parse_call_like(p: &mut RParser, lhs: CompletedMarker) -> Option<CompletedMarker> {
    let (open, close, arguments_kind, call_kind) = match p.cur() {
        L_PAREN => (L_PAREN, R_PAREN, R_CALL_ARGUMENTS, R_CALL),
        L_BRACK => (L_BRACK, R_BRACK, R_SUBSET_ARGUMENTS, R_SUBSET),
        L_BRACK2 => (L_BRACK2, R_BRACK2, R_SUBSET2_ARGUMENTS, R_SUBSET2),
        _ => unreachable!("Caller checked for an open delimiter"),
    };

    let m = lhs.precede(p);
    let arguments = p.start();
    p.bump(open);
    p.push_significance(false);

    let list = p.start();
    let ok = parse_argument_list(p, close);
    if ok {
        list.complete(p, R_ARGUMENT_LIST);
    } else {
        list.abandon(p);
    }

    let ok = ok && expect_close(p, close);
    p.pop_significance();

    if !ok {
        arguments.abandon(p);
        m.abandon(p);
        return None;
    }

    arguments.complete(p, arguments_kind);
    Some(m.complete(p, call_kind))
}

/// The `]]` of a subset2 is one token, but the lexer only ever produces
/// single `]` (in `x[y[1]]` the two `]` close different nodes). Gluing two
/// *adjacent* `]` also rejects `x[[1] ]` exactly like tree-sitter does.
fn expect_close(p: &mut RParser, close: RSyntaxKind) -> bool {
    if close == R_BRACK2 {
        if !p.try_glue_right_bracket2() {
            expected(p, "`]]`");
            return false;
        }
        p.bump(R_BRACK2);
        true
    } else {
        p.expect(close)
    }
}

/// Is the current token the closing delimiter? For subset2 a single `]` is
/// the start of the close: nothing else can follow an argument list.
fn at_close(p: &RParser, close: RSyntaxKind) -> bool {
    if close == R_BRACK2 {
        p.at(R_BRACK)
    } else {
        p.at(close)
    }
}

fn parse_argument_list(p: &mut RParser, close: RSyntaxKind) -> bool {
    let mut previous = Previous::Open;

    loop {
        if at_close(p, close) || p.at(EOF) {
            if previous == Previous::Comma {
                empty_argument(p);
            }
            // A missing close delimiter is reported by `expect_close`
            return true;
        }

        if p.at(COMMA) {
            if previous != Previous::Argument {
                empty_argument(p);
            }
            p.bump(COMMA);
            previous = Previous::Comma;
            continue;
        }

        if previous == Previous::Argument {
            // Two arguments must be separated by a comma: `fn(a b)` is an error
            expected(p, "`,` or the end of the arguments");
            return false;
        }

        if parse_argument(p, close).is_none() {
            return false;
        }
        previous = Previous::Argument;
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Previous {
    Open,
    Comma,
    Argument,
}

/// A zero-width `R_ARGUMENT` node marking a hole
fn empty_argument(p: &mut RParser) {
    let m = p.start();
    m.complete(p, R_ARGUMENT);
}

fn parse_argument(p: &mut RParser, close: RSyntaxKind) -> Option<CompletedMarker> {
    let m = p.start();

    let ok = if at_argument_name_start(p.cur()) {
        // Parse the would-be name; a following `=` turns it into a name
        // clause, otherwise it continues as an ordinary value expression
        match parse_argument_name(p) {
            None => false,
            Some(name) => {
                if p.at(EQUAL) {
                    let clause = name.precede(p);
                    p.bump(EQUAL);
                    clause.complete(p, R_ARGUMENT_NAME_CLAUSE);
                    // The value is optional: `fn(a = )` is valid
                    if p.at(COMMA) || at_close(p, close) || p.at(EOF) {
                        true
                    } else {
                        parse_expression(p, 0).is_some()
                    }
                } else {
                    parse_expression_rest(p, name, 0).is_some()
                }
            }
        }
    } else {
        parse_expression(p, 0).is_some()
    };

    if !ok {
        m.abandon(p);
        return None;
    }
    Some(m.complete(p, R_ARGUMENT))
}

fn at_argument_name_start(kind: RSyntaxKind) -> bool {
    matches!(kind, IDENT | STRING_OPEN | DOTS | DOTDOTI | NULL_KW)
}

fn parse_argument_name(p: &mut RParser) -> Option<CompletedMarker> {
    match p.cur() {
        STRING_OPEN => atoms::parse_string(p),
        _ => atoms::parse_value(p),
    }
}
