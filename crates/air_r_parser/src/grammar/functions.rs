//! Function definitions: `function(x, y = 1) body` and `\(x) body`
//!
//! Shape: `R_FUNCTION_DEFINITION { name, R_PARAMETERS { '(',
//! R_PARAMETER_LIST [ R_PARAMETER { name, R_PARAMETER_DEFAULT? } ], ')' },
//! body }`. The `R_PARAMETER_LIST` wrapper and `R_PARAMETER_DEFAULT` grouping
//! are synthesized relative to tree-sitter's flat shape.

use air_r_syntax::RSyntaxKind::*;
use biome_parser::Parser;
use biome_parser::prelude::CompletedMarker;

use crate::grammar::atoms;
use crate::grammar::expected;
use crate::grammar::expressions::FUNCTION_BODY_BP;
use crate::grammar::expressions::parse_expression;
use crate::parser::RParser;

pub(crate) fn parse_function_definition(p: &mut RParser) -> Option<CompletedMarker> {
    let m = p.start();
    // `function` or `\`
    p.bump(p.cur());

    let Some(_) = parse_parameters(p) else {
        m.abandon(p);
        return None;
    };

    let Some(_) = parse_expression(p, FUNCTION_BODY_BP) else {
        m.abandon(p);
        return None;
    };

    Some(m.complete(p, R_FUNCTION_DEFINITION))
}

fn parse_parameters(p: &mut RParser) -> Option<CompletedMarker> {
    let m = p.start();

    if !p.expect(L_PAREN) {
        m.abandon(p);
        return None;
    }
    p.push_significance(false);

    let list = p.start();
    let ok = parse_parameter_list(p);
    if ok {
        list.complete(p, R_PARAMETER_LIST);
    } else {
        list.abandon(p);
    }

    let ok = ok && p.expect(R_PAREN);
    p.pop_significance();

    if !ok {
        m.abandon(p);
        return None;
    }
    Some(m.complete(p, R_PARAMETERS))
}

fn parse_parameter_list(p: &mut RParser) -> bool {
    // A comma requires another parameter after it: `function(x,)` is an error
    let mut needs_parameter = false;

    loop {
        if p.at(R_PAREN) {
            if needs_parameter {
                expected(p, "a parameter");
                return false;
            }
            return true;
        }

        if parse_parameter(p).is_none() {
            return false;
        }

        if p.at(COMMA) {
            p.bump(COMMA);
            needs_parameter = true;
        } else {
            return true;
        }
    }
}

fn parse_parameter(p: &mut RParser) -> Option<CompletedMarker> {
    let m = p.start();

    let name_ok = match p.cur() {
        DOTS | DOTDOTI => {
            let name = if p.at(DOTS) { R_DOTS } else { R_DOT_DOT_I };
            let inner = p.start();
            p.bump(p.cur());
            inner.complete(p, name);
            true
        }
        // Identifier, or a keyword demoted to one (`function(if) 1` is valid)
        _ => atoms::parse_identifier_lax(p).is_some(),
    };

    if !name_ok {
        m.abandon(p);
        return None;
    }

    if p.at(EQUAL) {
        let default = p.start();
        p.bump(EQUAL);
        let Some(_) = parse_expression(p, 0) else {
            default.abandon(p);
            m.abandon(p);
            return None;
        };
        default.complete(p, R_PARAMETER_DEFAULT);
    }

    Some(m.complete(p, R_PARAMETER))
}
