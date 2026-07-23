//! The hand-written Pratt parser backend
//!
//! The tree shapes reproduce what the previous tree-sitter backend produced
//! (verified byte-identical over a ~12k file corpus before that backend was
//! removed): the *converted* shape its walker synthesized (expression lists,
//! argument holes, name clauses, ...), not tree-sitter's raw shape.
//!
//! Grammar functions return `Option<CompletedMarker>`: `None` means a parse
//! error was recorded, so callers abandon their markers and propagate. The
//! propagation stops at the enclosing statement list, which wraps everything
//! the failed statement consumed in an `R_BOGUS_EXPRESSION`, skips ahead to
//! the next statement boundary, and keeps parsing — so a file with a syntax
//! error still gets a full tree for everything else, with `Parse::has_error()`
//! reporting that an error occurred. (The old backend collapsed the entire
//! file into a single bogus node instead.)

mod atoms;
mod calls;
mod control;
mod expressions;
mod functions;

use air_r_syntax::RSyntaxKind;
use biome_parser::Parser;
use biome_parser::event::Event;
use biome_parser::prelude::CompletedMarker;
use biome_parser::prelude::Marker;
use biome_parser::prelude::Trivia;

use crate::ParseError;
use crate::parser::RParser;

pub(crate) fn parse_text(text: &str) -> (Vec<Event<RSyntaxKind>>, Vec<Trivia>, Option<ParseError>) {
    let mut p = RParser::new(text);

    parse_root(&mut p);

    let failed = p.failed();
    let (events, trivia) = p.finish();

    let error =
        failed.then(|| ParseError::new(String::from("Failed to parse due to syntax errors.")));

    (events, trivia, error)
}

fn parse_root(p: &mut RParser) -> CompletedMarker {
    let m = p.start();

    // NOTE: the old tree-sitter backend panicked on a leading BOM; here it
    // fills the root's optional bom slot instead
    if p.at(RSyntaxKind::UNICODE_BOM) {
        p.bump(RSyntaxKind::UNICODE_BOM);
    }

    parse_expression_list(p, RSyntaxKind::EOF);

    // The tree sink adds the EOF token (with the remaining trivia) itself
    m.complete(p, RSyntaxKind::R_ROOT)
}

/// The statement list of the program or of a `{}` block, running until
/// `end` (`EOF` or `R_CURLY`)
///
/// This is the only place where semicolons are legal, so each iteration
/// blesses the separators preceding the upcoming token (see
/// `token_source.rs`).
///
/// It is also where errors are recovered, so this never fails: a marker is
/// opened before each statement, and if the statement errors, the marker —
/// wrapping every token the failed parse consumed, since inner markers were
/// abandoned into it — is completed as `R_BOGUS_EXPRESSION` after skipping to
/// a plausible statement boundary.
pub(crate) fn parse_expression_list(p: &mut RParser, end: RSyntaxKind) -> CompletedMarker {
    let m = p.start();

    loop {
        p.bless_semicolons();

        if p.at(end) || p.at(RSyntaxKind::EOF) {
            break;
        }

        let statement = p.start();
        match expressions::parse_expression(p, 0) {
            Some(_) => statement.abandon(p),
            None => recover_statement(p, statement, end),
        }
    }

    m.complete(p, RSyntaxKind::R_EXPRESSION_LIST)
}

/// Skip ahead to a plausible statement boundary: a token preceded by a line
/// break or semicolon, the end of the enclosing list, or EOF. We don't use
/// biome's `ParseRecoveryTokenSet` because its "recover on line break" can't
/// see R's semicolon separators (they are folded into whitespace trivia).
fn recover_statement(p: &mut RParser, m: Marker, end: RSyntaxKind) {
    // Did the failed statement consume anything before erroring?
    let mut made_progress = p.cur_range().start() != m.start();

    loop {
        if p.at(end) || p.at(RSyntaxKind::EOF) {
            break;
        }

        // Only stop on a separator once we've made progress, otherwise a
        // failure on a separator-preceded token would recover in place and
        // the list loop would try the same token again, forever
        if made_progress && p.has_preceding_separator() {
            break;
        }

        p.bump(p.cur());
        made_progress = true;
    }

    m.complete(p, RSyntaxKind::R_BOGUS_EXPRESSION);
}

/// Record an error diagnostic at the current token
fn expected(p: &mut RParser, what: &str) {
    let range = p.cur_range();
    let diagnostic = p.err_builder(format!("expected {what}"), range);
    p.error(diagnostic);
}
