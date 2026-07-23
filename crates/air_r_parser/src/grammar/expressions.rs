//! The Pratt core: expressions with binding powers
//!
//! Binding powers are `2 * rank` with the ranks taken from tree-sitter-r's
//! `grammar.js` precedence table, so both backends resolve every conflict the
//! same way. Left-associative operators get `(2r, 2r + 1)`, right-associative
//! ones `(2r + 1, 2r)`.
//!
//! Newline significance: a line break or semicolon before an infix/postfix
//! operator ends the expression in newline-significant contexts (top level
//! and inside `{}`), which is how `1\n+2` is two statements while `(1\n+2)`
//! is one. Newlines *after* an operator are always allowed by the grammar, so
//! the check only happens here, before consuming an operator.

use air_r_syntax::RSyntaxKind;
use air_r_syntax::RSyntaxKind::*;
use biome_parser::Parser;
use biome_parser::prelude::CompletedMarker;

use crate::grammar::atoms;
use crate::grammar::calls;
use crate::grammar::control;
use crate::grammar::expected;
use crate::grammar::functions;
use crate::parser::RParser;

/// Left binding power of `$` / `@` (rank 18)
const EXTRACT_LBP: u8 = 36;
/// Left binding power of `::` / `:::` (rank 19)
const NAMESPACE_LBP: u8 = 38;
/// Left binding power of call/subset/subset2 (rank 20)
const CALL_LBP: u8 = 40;

/// Minimum binding power for the body of `function`/`for`/`while`/`repeat`
/// (rank 2): every infix operator except `?` (rank 1) binds into the body
pub(crate) const FUNCTION_BODY_BP: u8 = 4;
/// Minimum binding power for `if` consequence/alternative (rank 3)
pub(crate) const IF_BODY_BP: u8 = 6;

fn binary_binding_power(kind: RSyntaxKind) -> Option<(u8, u8)> {
    Some(match kind {
        // `?` (rank 1, left)
        WAT => (2, 3),
        // `<-` `<<-` `:=` (rank 4, right)
        ASSIGN | SUPER_ASSIGN | WALRUS => (9, 8),
        // `=` (rank 5, right)
        EQUAL => (11, 10),
        // `->` `->>` (rank 6, left)
        ASSIGN_RIGHT | SUPER_ASSIGN_RIGHT => (12, 13),
        // `~` (rank 7, left)
        TILDE => (14, 15),
        // `|` `||` (rank 8, left)
        OR | OR2 => (16, 17),
        // `&` `&&` (rank 9, left)
        AND | AND2 => (18, 19),
        // comparisons (rank 11, left)
        LESS_THAN
        | LESS_THAN_OR_EQUAL_TO
        | GREATER_THAN
        | GREATER_THAN_OR_EQUAL_TO
        | EQUAL2
        | NOT_EQUAL => (22, 23),
        // `+` `-` (rank 12, left)
        PLUS | MINUS => (24, 25),
        // `*` `/` (rank 13, left)
        MULTIPLY | DIVIDE => (26, 27),
        // `%op%` `|>` (rank 14, left)
        SPECIAL | PIPE => (28, 29),
        // `:` (rank 15, left)
        COLON => (30, 31),
        // `^` `**` (rank 17, right)
        EXPONENTIATE | EXPONENTIATE2 => (35, 34),
        _ => return None,
    })
}

/// Operand binding power of the prefix operators
///
/// All unary operators are `prec.left` in the grammar, so on a tie with the
/// equal-rank *binary* operator the unary one reduces first: `A + ~B + C ~ D`
/// is `(A + ~(B + C)) ~ D`. Hence `2 * rank + 1`, keeping the equal-rank
/// binary operator out of the operand.
fn unary_binding_power(kind: RSyntaxKind) -> Option<u8> {
    Some(match kind {
        // `?` (rank 1)
        WAT => 3,
        // `~` (rank 7)
        TILDE => 15,
        // `!` (rank 10)
        BANG => 21,
        // unary `+` `-` (rank 16)
        PLUS | MINUS => 33,
        _ => return None,
    })
}

/// Remaining stack below which we allocate a new segment. Deeply nested
/// R (e.g. generated code full of `(((...)))`) recurses through
/// [parse_expression]; tree-sitter handled arbitrary nesting with an explicit
/// heap stack, so instead of overflowing (or imposing a depth limit that
/// would reject files tree-sitter accepted) we grow the stack on demand, the
/// same way rustc and rust-analyzer do.
const STACK_RED_ZONE: usize = 128 * 1024;
const STACK_GROW_SIZE: usize = 4 * 1024 * 1024;

pub(crate) fn parse_expression(p: &mut RParser, min_bp: u8) -> Option<CompletedMarker> {
    stacker::maybe_grow(STACK_RED_ZONE, STACK_GROW_SIZE, || {
        let lhs = parse_prefix(p)?;
        parse_expression_rest(p, lhs, min_bp)
    })
}

/// Continue an expression from an already-parsed `lhs` (the Pratt led loop)
///
/// Also a stack-growth point: recursion can cycle through here without
/// passing [parse_expression] (`parse_argument` continues from a pre-parsed
/// name atom, so `x[[y[y[y[...` never enters [parse_expression] between
/// levels).
pub(crate) fn parse_expression_rest(
    p: &mut RParser,
    lhs: CompletedMarker,
    min_bp: u8,
) -> Option<CompletedMarker> {
    stacker::maybe_grow(STACK_RED_ZONE, STACK_GROW_SIZE, || {
        parse_expression_rest_inner(p, lhs, min_bp)
    })
}

fn parse_expression_rest_inner(
    p: &mut RParser,
    mut lhs: CompletedMarker,
    min_bp: u8,
) -> Option<CompletedMarker> {
    loop {
        let kind = p.cur();

        // Determine the operator's left binding power, or stop if the current
        // token can't continue the expression
        let (lbp, rbp) = match kind {
            L_PAREN | L_BRACK | L_BRACK2 => (CALL_LBP, 0),
            DOLLAR | AT => (EXTRACT_LBP, 0),
            COLON2 | COLON3 => (NAMESPACE_LBP, 0),
            _ => match binary_binding_power(kind) {
                Some(power) => power,
                None => break,
            },
        };

        if lbp < min_bp {
            break;
        }

        // A statement separator before the operator ends the expression in
        // newline-significant contexts
        if p.newlines_significant() && p.has_preceding_separator() {
            break;
        }

        lhs = match kind {
            L_PAREN | L_BRACK | L_BRACK2 => calls::parse_call_like(p, lhs)?,
            DOLLAR | AT => parse_extract(p, lhs)?,
            COLON2 | COLON3 => parse_namespace(p, lhs)?,
            _ => {
                let m = lhs.precede(p);
                p.bump(kind);
                let Some(_) = parse_expression(p, rbp) else {
                    m.abandon(p);
                    return None;
                };
                m.complete(p, R_BINARY_EXPRESSION)
            }
        };
    }

    Some(lhs)
}

fn parse_prefix(p: &mut RParser) -> Option<CompletedMarker> {
    if let Some(operand_bp) = unary_binding_power(p.cur()) {
        let m = p.start();
        p.bump(p.cur());
        let Some(_) = parse_expression(p, operand_bp) else {
            m.abandon(p);
            return None;
        };
        return Some(m.complete(p, R_UNARY_EXPRESSION));
    }

    match p.cur() {
        L_PAREN => control::parse_parenthesized(p),
        L_CURLY => control::parse_braces(p),
        FUNCTION_KW | BACKSLASH => functions::parse_function_definition(p),
        IF_KW => control::parse_if(p),
        FOR_KW => control::parse_for(p),
        WHILE_KW => control::parse_while(p),
        REPEAT_KW => control::parse_repeat(p),
        kind if atoms::at_value_start(kind) => atoms::parse_value(p),
        _ => {
            expected(p, "an expression");
            None
        }
    }
}

/// `x$y` / `x@y`: any expression on the left, an optional selector on the
/// right (`x$` is a valid parse in tree-sitter-r, the slot is just missing)
fn parse_extract(p: &mut RParser, lhs: CompletedMarker) -> Option<CompletedMarker> {
    let m = lhs.precede(p);
    p.bump(p.cur());

    // The selector is optional, and empirically (tree-sitter is the arbiter):
    // - a semicolon always blocks it (`x$;y`)
    // - newlines never block it: `$` keeps looking for its selector across any
    //   number of blank lines, so `x$\n\ny` is still `x$y`. Only a semicolon
    //   terminates the search.
    if atoms::at_selector_start(p.cur()) && !p.has_preceding_semicolons() {
        let Some(_) = atoms::parse_selector(p) else {
            m.abandon(p);
            return None;
        };
    }

    Some(m.complete(p, R_EXTRACT_EXPRESSION))
}

/// `pkg::name` / `pkg:::name`: selectors on both sides, right side optional.
/// Unlike `$`, the grammar does not allow newlines after `::`, so in
/// newline-significant contexts a line break leaves the right side missing.
fn parse_namespace(p: &mut RParser, lhs: CompletedMarker) -> Option<CompletedMarker> {
    if !atoms::is_selector_kind(lhs.kind(p)) {
        let operator = if p.at(COLON3) { ":::" } else { "::" };
        expected(p, &format!("an identifier or string before `{operator}`"));
        return None;
    }

    let m = lhs.precede(p);
    p.bump(p.cur());

    let separator_blocks_rhs = p.newlines_significant() && p.has_preceding_separator();
    if atoms::at_selector_start(p.cur()) && !separator_blocks_rhs && !p.has_preceding_semicolons() {
        let Some(_) = atoms::parse_selector(p) else {
            m.abandon(p);
            return None;
        };
    }

    Some(m.complete(p, R_NAMESPACE_EXPRESSION))
}
