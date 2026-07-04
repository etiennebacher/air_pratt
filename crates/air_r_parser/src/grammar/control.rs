//! Control flow and grouping: `if`/`else`, `for`, `while`, `repeat`, `{}`, `()`

use air_r_syntax::RSyntaxKind::*;
use biome_parser::Parser;
use biome_parser::prelude::CompletedMarker;

use crate::grammar::atoms;
use crate::grammar::expressions::FUNCTION_BODY_BP;
use crate::grammar::expressions::IF_BODY_BP;
use crate::grammar::expressions::parse_expression;
use crate::grammar::parse_expression_list;
use crate::parser::RParser;

/// `( body )` — exactly one expression, newlines insignificant inside
pub(crate) fn parse_parenthesized(p: &mut RParser) -> Option<CompletedMarker> {
    let m = p.start();
    p.bump(L_PAREN);
    p.push_significance(false);

    let body = parse_expression(p, 0);
    let ok = body.is_some() && p.expect(R_PAREN);

    p.pop_significance();
    if !ok {
        m.abandon(p);
        return None;
    }
    Some(m.complete(p, R_PARENTHESIZED_EXPRESSION))
}

/// `{ expressions }` — a statement list, newlines significant inside
pub(crate) fn parse_braces(p: &mut RParser) -> Option<CompletedMarker> {
    let m = p.start();
    p.bump(L_CURLY);
    p.push_significance(true);

    // Never fails: errors inside the block recover per-statement
    parse_expression_list(p, R_CURLY);
    let ok = p.expect(R_CURLY);

    p.pop_significance();
    if !ok {
        m.abandon(p);
        return None;
    }
    Some(m.complete(p, R_BRACED_EXPRESSIONS))
}

/// `if (condition) consequence [else alternative]`
pub(crate) fn parse_if(p: &mut RParser) -> Option<CompletedMarker> {
    let m = p.start();
    p.bump(IF_KW);

    if !parse_condition(p) {
        m.abandon(p);
        return None;
    }

    let Some(_) = parse_expression(p, IF_BODY_BP) else {
        m.abandon(p);
        return None;
    };

    // `else` is a contextual keyword lexed as IDENT; it only becomes a
    // keyword right here. It binds across a line break anywhere except the
    // top level (the tree-sitter scanner suppresses the newline before `else`
    // inside brackets), and a semicolon always ends the statement first — in
    // both of those cases the `else` is left alone and parses as a plain
    // identifier statement, exactly like tree-sitter.
    let else_allowed = p.at(IDENT)
        && p.cur_text() == "else"
        && !p.has_preceding_semicolons()
        && (!p.has_preceding_separator() || p.inside_brackets());

    if else_allowed {
        let clause = p.start();
        p.bump_remap(ELSE_KW);
        let Some(_) = parse_expression(p, IF_BODY_BP) else {
            clause.abandon(p);
            m.abandon(p);
            return None;
        };
        clause.complete(p, R_ELSE_CLAUSE);
    }

    Some(m.complete(p, R_IF_STATEMENT))
}

/// `for (variable in sequence) body`
pub(crate) fn parse_for(p: &mut RParser) -> Option<CompletedMarker> {
    let m = p.start();
    p.bump(FOR_KW);

    if !p.expect(L_PAREN) {
        m.abandon(p);
        return None;
    }
    p.push_significance(false);

    let ok = atoms::parse_identifier_lax(p).is_some()
        && expect_in(p)
        && parse_expression(p, 0).is_some()
        && p.expect(R_PAREN);

    p.pop_significance();
    if !ok {
        m.abandon(p);
        return None;
    }

    let Some(_) = parse_expression(p, FUNCTION_BODY_BP) else {
        m.abandon(p);
        return None;
    };

    Some(m.complete(p, R_FOR_STATEMENT))
}

/// `while (condition) body`
pub(crate) fn parse_while(p: &mut RParser) -> Option<CompletedMarker> {
    let m = p.start();
    p.bump(WHILE_KW);

    if !parse_condition(p) {
        m.abandon(p);
        return None;
    }

    let Some(_) = parse_expression(p, FUNCTION_BODY_BP) else {
        m.abandon(p);
        return None;
    };

    Some(m.complete(p, R_WHILE_STATEMENT))
}

/// `repeat body`
pub(crate) fn parse_repeat(p: &mut RParser) -> Option<CompletedMarker> {
    let m = p.start();
    p.bump(REPEAT_KW);

    let Some(_) = parse_expression(p, FUNCTION_BODY_BP) else {
        m.abandon(p);
        return None;
    };

    Some(m.complete(p, R_REPEAT_STATEMENT))
}

/// The contextual `in` keyword of a `for` loop (lexed as IDENT)
fn expect_in(p: &mut RParser) -> bool {
    if p.at(IDENT) && p.cur_text() == "in" {
        p.bump_remap(IN_KW);
        true
    } else {
        crate::grammar::expected(p, "`in`");
        false
    }
}

/// The `( condition )` of `if` and `while`: direct children of the statement
/// node, not a parenthesized expression
fn parse_condition(p: &mut RParser) -> bool {
    if !p.expect(L_PAREN) {
        return false;
    }
    p.push_significance(false);

    let ok = parse_expression(p, 0).is_some() && p.expect(R_PAREN);

    p.pop_significance();
    ok
}
