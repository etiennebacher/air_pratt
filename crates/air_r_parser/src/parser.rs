//! The parser type for the Pratt backend
//!
//! Wraps [ParserContext] (events, diagnostics, markers) and [RTokenSource],
//! and tracks R's newline significance: a line break (or semicolon) before an
//! infix operator terminates the expression at top level and inside `{}`, but
//! not inside `(`, `[`, or `[[` (see `grammar/expressions.rs`).

use air_r_syntax::RSyntaxKind;
use biome_parser::ParserContext;
use biome_parser::diagnostic::ParseDiagnostic;
use biome_parser::event::Event;
use biome_parser::prelude::TokenSource;
use biome_parser::prelude::Trivia;

use crate::token_source::RTokenSource;

pub(crate) struct RParser<'src> {
    context: ParserContext<RSyntaxKind>,
    source: RTokenSource<'src>,
    /// Newline significance per enclosing bracket: `true` for the root frame
    /// and `{}` frames, `false` for `(`/`[`/`[[` frames
    significance: Vec<bool>,
}

impl<'src> RParser<'src> {
    pub(crate) fn new(text: &'src str) -> Self {
        Self {
            context: ParserContext::default(),
            source: RTokenSource::new(text),
            significance: vec![true],
        }
    }

    /// Are newlines/semicolons statement terminators in the current context?
    pub(crate) fn newlines_significant(&self) -> bool {
        *self.significance.last().unwrap()
    }

    /// True inside any `(`, `[`, `[[`, or `{}` — i.e. not at the top level.
    /// R allows `else` after a line break anywhere but the top level.
    pub(crate) fn inside_brackets(&self) -> bool {
        self.significance.len() > 1
    }

    pub(crate) fn push_significance(&mut self, significant: bool) {
        self.significance.push(significant);
    }

    pub(crate) fn pop_significance(&mut self) {
        self.significance.pop();
    }

    /// Line break or semicolon before the current token
    pub(crate) fn has_preceding_separator(&self) -> bool {
        self.source.has_preceding_separator()
    }

    pub(crate) fn has_preceding_semicolons(&self) -> bool {
        self.source.gap_semicolons() > 0
    }

    /// Number of line breaks before the current token
    pub(crate) fn preceding_newlines(&self) -> u32 {
        self.source.gap_newlines()
    }

    pub(crate) fn bless_semicolons(&mut self) {
        self.source.bless_semicolons();
    }

    pub(crate) fn try_glue_right_bracket2(&mut self) -> bool {
        self.source.try_glue_right_bracket2()
    }

    /// Finish parsing, returning the events and trivia for tree building along
    /// with every diagnostic (message + range) recorded by the grammar, lexer,
    /// and token source.
    pub(crate) fn finish(self) -> (Vec<Event<RSyntaxKind>>, Vec<Trivia>, Vec<ParseDiagnostic>) {
        let (events, mut diagnostics) = self.context.finish();
        let (trivia, source_diagnostics) = self.source.finish();
        diagnostics.extend(source_diagnostics);
        (events, trivia, diagnostics)
    }
}

impl<'src> biome_parser::Parser for RParser<'src> {
    type Kind = RSyntaxKind;
    type Source = RTokenSource<'src>;

    fn context(&self) -> &ParserContext<RSyntaxKind> {
        &self.context
    }

    fn context_mut(&mut self) -> &mut ParserContext<RSyntaxKind> {
        &mut self.context
    }

    fn source(&self) -> &RTokenSource<'src> {
        &self.source
    }

    fn source_mut(&mut self) -> &mut RTokenSource<'src> {
        &mut self.source
    }
}
