//! Leaf expressions: identifiers, literals, strings, and keyword constants
//!
//! Every leaf is a node wrapping a token (`R_IDENTIFIER { IDENT }`,
//! `R_TRUE_EXPRESSION { TRUE_KW }`, ...) per the ungrammar, matching what the
//! tree-sitter walker synthesizes from tree-sitter's bare leaf nodes.

use air_r_syntax::RSyntaxKind;
use air_r_syntax::RSyntaxKind::*;
use biome_parser::Parser;
use biome_parser::prelude::CompletedMarker;

use crate::grammar::expected;
use crate::parser::RParser;

pub(crate) fn at_value_start(kind: RSyntaxKind) -> bool {
    matches!(
        kind,
        IDENT
            | DOTS
            | DOTDOTI
            | R_DOUBLE_LITERAL
            | R_INTEGER_LITERAL
            | R_COMPLEX_LITERAL
            | STRING_OPEN
            | TRUE_KW
            | FALSE_KW
            | NULL_KW
            | INF_KW
            | NAN_KW
            | NA_LOGICAL_KW
            | NA_INTEGER_KW
            | NA_DOUBLE_KW
            | NA_COMPLEX_KW
            | NA_CHARACTER_KW
            | NEXT_KW
            | BREAK_KW
    )
}

/// A selector: the restricted operand of `$`, `@`, `::`, `:::`
pub(crate) fn at_selector_start(kind: RSyntaxKind) -> bool {
    matches!(kind, IDENT | STRING_OPEN | DOTS | DOTDOTI)
}

/// Is a completed node usable as the left side of `::`/`:::`?
pub(crate) fn is_selector_kind(kind: RSyntaxKind) -> bool {
    matches!(kind, R_IDENTIFIER | R_STRING_VALUE | R_DOTS | R_DOT_DOT_I)
}

pub(crate) fn parse_value(p: &mut RParser) -> Option<CompletedMarker> {
    if p.at(STRING_OPEN) {
        return parse_string(p);
    }

    let node = match p.cur() {
        IDENT => R_IDENTIFIER,
        DOTS => R_DOTS,
        DOTDOTI => R_DOT_DOT_I,
        R_DOUBLE_LITERAL => R_DOUBLE_VALUE,
        R_INTEGER_LITERAL => R_INTEGER_VALUE,
        R_COMPLEX_LITERAL => R_COMPLEX_VALUE,
        TRUE_KW => R_TRUE_EXPRESSION,
        FALSE_KW => R_FALSE_EXPRESSION,
        NULL_KW => R_NULL_EXPRESSION,
        INF_KW => R_INF_EXPRESSION,
        NAN_KW => R_NAN_EXPRESSION,
        NA_LOGICAL_KW | NA_INTEGER_KW | NA_DOUBLE_KW | NA_COMPLEX_KW | NA_CHARACTER_KW => {
            R_NA_EXPRESSION
        }
        NEXT_KW => R_NEXT_EXPRESSION,
        BREAK_KW => R_BREAK_EXPRESSION,
        _ => {
            expected(p, "a value");
            return None;
        }
    };

    let m = p.start();
    p.bump(p.cur());
    Some(m.complete(p, node))
}

/// `R_STRING_VALUE { STRING_OPEN, STRING_CONTENT?, STRING_CLOSE }`
pub(crate) fn parse_string(p: &mut RParser) -> Option<CompletedMarker> {
    let m = p.start();
    p.bump(STRING_OPEN);

    if p.at(STRING_CONTENT) {
        p.bump(STRING_CONTENT);
    }

    if !p.at(STRING_CLOSE) {
        // Notably on lexer errors (unterminated string)
        expected(p, "the end of the string");
        m.abandon(p);
        return None;
    }
    p.bump(STRING_CLOSE);

    Some(m.complete(p, R_STRING_VALUE))
}

pub(crate) fn parse_selector(p: &mut RParser) -> Option<CompletedMarker> {
    match p.cur() {
        IDENT | DOTS | DOTDOTI => parse_value(p),
        STRING_OPEN => parse_string(p),
        _ => {
            expected(p, "an identifier or string");
            None
        }
    }
}

/// `R_IDENTIFIER { IDENT }` in positions where *only* an identifier is
/// valid (`for` loop variables, parameter names): tree-sitter's keyword
/// extraction falls back to an identifier there, so `for (function in x)` and
/// `function(if) 1` both parse, with the keyword demoted to a name.
pub(crate) fn parse_identifier_lax(p: &mut RParser) -> Option<CompletedMarker> {
    if p.at(IDENT) {
        let m = p.start();
        p.bump(IDENT);
        return Some(m.complete(p, R_IDENTIFIER));
    }
    if is_keyword(p.cur()) {
        let m = p.start();
        p.bump_remap(IDENT);
        return Some(m.complete(p, R_IDENTIFIER));
    }
    expected(p, "an identifier");
    None
}

fn is_keyword(kind: RSyntaxKind) -> bool {
    matches!(
        kind,
        FUNCTION_KW
            | IF_KW
            | ELSE_KW
            | FOR_KW
            | IN_KW
            | WHILE_KW
            | REPEAT_KW
            | NEXT_KW
            | BREAK_KW
            | TRUE_KW
            | FALSE_KW
            | NULL_KW
            | INF_KW
            | NAN_KW
            | NA_LOGICAL_KW
            | NA_INTEGER_KW
            | NA_DOUBLE_KW
            | NA_COMPLEX_KW
            | NA_CHARACTER_KW
    )
}
