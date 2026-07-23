//! Token source for the Pratt backend
//!
//! Drives [RLexer] and converts the trivia between non-trivia tokens into
//! [Trivia] pieces. The conversion must be byte-identical to what the
//! tree-sitter backend's `derive_trivia` + comment handling produce (see the
//! rules on [TriviaState]), because the differential test compares whole
//! green trees.
//!
//! Semicolons are R statement separators, but the tree-sitter backend never
//! sees them as tokens and folds them into `Whitespace` trivia pieces (see the
//! `TODO(semicolon)` on `derive_trivia`). We reproduce that: `;` is skipped
//! into the surrounding whitespace piece. To still *reject* semicolons in
//! places where tree-sitter errors (e.g. `f(a;)`), each skipped semicolon is
//! remembered, and the grammar's statement-list loops "bless" the ones that sit
//! at legal statement boundaries. Any semicolon left unblessed once we leave its
//! gap becomes a stray-semicolon diagnostic, which is exactly the parity failure
//! mode.

use air_r_syntax::RSyntaxKind;
use biome_parser::diagnostic::ParseDiagnostic;
use biome_parser::prelude::TokenSource;
use biome_parser::prelude::Trivia;
use biome_rowan::TextRange;
use biome_rowan::TextSize;
use biome_rowan::TriviaPieceKind;

use crate::lexer::RLexer;
use crate::lexer::Token;

pub(crate) struct RTokenSource<'src> {
    text: &'src str,
    lexer: RLexer<'src>,
    /// One-token lexer lookahead, used for gluing `]` `]` into `]]`
    lookahead: Option<Token>,
    current: Token,
    trivia: TriviaState,
    /// Number of *real* line breaks (`\n` or `\r\n`) in the gap before
    /// `current`. A lone `\r` is not a line break to tree-sitter (its scanner
    /// skips it as plain whitespace), so it doesn't count, even though it
    /// still becomes a `Newline` trivia piece. The count matters because an
    /// extract selector (`x$\ny`) survives exactly one line break.
    gap_newlines: u32,
    /// Number of semicolons in the gap before `current`
    gap_semicolons: u32,
    /// Range of the first semicolon in the current gap that hasn't been blessed
    /// as a statement separator yet. Cleared when the gap is blessed, promoted
    /// to `stray_semicolon` when we leave the gap without blessing it.
    pending_semicolon: Option<TextRange>,
    /// Range of the first stray semicolon: one that sat somewhere other than a
    /// statement boundary (e.g. `f(a;)`), which tree-sitter rejects
    stray_semicolon: Option<TextRange>,
}

/// Converts the raw text between non-trivia tokens into trivia pieces
///
/// This is a port of the tree-sitter backend's `derive_trivia` and
/// `handle_comment_leave` (`parse.rs`), keyed on the same state:
///
/// - Before the first token and before EOF, everything is leading.
/// - Between two tokens, everything is leading, except that when the gap
///   contains a newline, the whitespace run before the first newline is
///   trailing (a gap with no newline is one *leading* whitespace piece).
/// - A comment is trailing iff there is no `\n` between it and the previous
///   token (and we're past the first token); the whitespace before a trailing
///   comment is a trailing piece. Comments don't affect `between_two_tokens`.
/// - Whitespace runs (including semicolons) form single merged pieces; each
///   `\n`, `\r\n`, or lone `\r` is its own `Newline` piece.
struct TriviaState {
    pieces: Vec<Trivia>,
    /// End of the previous non-trivia token or comment
    last_end: TextSize,
    /// False before the first non-trivia token; forced false before EOF
    between_two_tokens: bool,
}

impl TriviaState {
    fn new() -> Self {
        Self {
            pieces: Vec::new(),
            last_end: TextSize::from(0),
            between_two_tokens: false,
        }
    }

    fn push(&mut self, kind: TriviaPieceKind, range: TextRange, trailing: bool) {
        self.pieces.push(Trivia::new(kind, range, trailing));
    }

    /// Process the whitespace/newline/semicolon gap in `text[self.last_end..end]`
    fn derive_gap(&mut self, text: &str, end: TextSize, between_two_tokens: bool) {
        let mut start = self.last_end;
        let bytes = &text.as_bytes()[usize::from(start)..usize::from(end)];
        let mut iter = bytes.iter().copied().peekable();

        if between_two_tokens {
            let mut trailing = false;
            let mut run_end = start;
            while let Some(byte) = iter.peek() {
                if let b'\r' | b'\n' = byte {
                    trailing = true;
                    break;
                }
                run_end += TextSize::from(1);
                iter.next();
            }
            if start != run_end {
                self.push(
                    TriviaPieceKind::Whitespace,
                    TextRange::new(start, run_end),
                    trailing,
                );
                start = run_end;
            }
        }

        // Everything from here on is leading
        while let Some(byte) = iter.next() {
            let piece_start = start;
            start += TextSize::from(1);

            match byte {
                b'\r' => {
                    if iter.peek() == Some(&b'\n') {
                        iter.next();
                        start += TextSize::from(1);
                    }
                    self.push(
                        TriviaPieceKind::Newline,
                        TextRange::new(piece_start, start),
                        false,
                    );
                }
                b'\n' => {
                    self.push(
                        TriviaPieceKind::Newline,
                        TextRange::new(piece_start, start),
                        false,
                    );
                }
                _ => {
                    // Whitespace (including semicolons): finish out the run
                    while iter
                        .next_if(|byte| !matches!(byte, b'\r' | b'\n'))
                        .is_some()
                    {
                        start += TextSize::from(1);
                    }
                    self.push(
                        TriviaPieceKind::Whitespace,
                        TextRange::new(piece_start, start),
                        false,
                    );
                }
            }
        }
    }

    fn handle_comment(&mut self, text: &str, range: TextRange) {
        let gap = &text[usize::from(self.last_end)..usize::from(range.start())];

        let trailing = if gap.contains('\n') {
            // A newline before the comment makes it leading
            self.derive_gap(text, range.start(), self.between_two_tokens);
            false
        } else {
            // Same line as the previous token: the comment and the whitespace
            // before it are trailing, unless we're at the start of the file
            if self.last_end != range.start() {
                self.push(
                    TriviaPieceKind::Whitespace,
                    TextRange::new(self.last_end, range.start()),
                    self.between_two_tokens,
                );
            }
            self.between_two_tokens
        };

        self.push(TriviaPieceKind::SingleLineComment, range, trailing);
        self.last_end = range.end();
    }

    fn handle_token(&mut self, text: &str, range: TextRange) {
        self.derive_gap(text, range.start(), self.between_two_tokens);
        self.last_end = range.end();
        self.between_two_tokens = true;
    }

    fn handle_eof(&mut self, text: &str) {
        // All end-of-file trivia leads the EOF token the tree sink adds
        let end = TextSize::try_from(text.len()).unwrap();
        self.derive_gap(text, end, false);
        self.last_end = end;
    }
}

impl<'src> RTokenSource<'src> {
    pub(crate) fn new(text: &'src str) -> Self {
        let eof = TextSize::try_from(text.len()).unwrap();
        let mut source = Self {
            text,
            lexer: RLexer::new(text),
            lookahead: None,
            current: Token::new(RSyntaxKind::EOF, TextRange::new(eof, eof)),
            trivia: TriviaState::new(),
            gap_newlines: 0,
            gap_semicolons: 0,
            pending_semicolon: None,
            stray_semicolon: None,
        };
        source.advance();
        source
    }

    fn next_lexer_token(&mut self) -> Option<Token> {
        match self.lookahead.take() {
            Some(token) => Some(token),
            None => self.lexer.next_token(),
        }
    }

    fn peek_lexer_token(&mut self) -> Option<Token> {
        if self.lookahead.is_none() {
            self.lookahead = self.lexer.next_token();
        }
        self.lookahead
    }

    /// Advance `current` to the next non-trivia token, converting the trivia
    /// in between
    fn advance(&mut self) {
        // We're leaving the current gap for good: no later `bless_semicolons`
        // can reach it, so a semicolon still pending here was never a statement
        // separator and is therefore stray.
        self.finalize_pending_semicolon();
        self.gap_newlines = 0;
        self.gap_semicolons = 0;
        self.pending_semicolon = None;

        loop {
            match self.next_lexer_token() {
                None => {
                    self.trivia.handle_eof(self.text);
                    let eof = TextSize::try_from(self.text.len()).unwrap();
                    self.current = Token::new(RSyntaxKind::EOF, TextRange::new(eof, eof));
                    return;
                }
                Some(token) => match token.kind() {
                    RSyntaxKind::WHITESPACE => {}
                    RSyntaxKind::NEWLINE => {
                        // Lone `\r` line endings don't count, see `gap_newlines`
                        if self.text.as_bytes()[usize::from(token.range().end()) - 1] == b'\n' {
                            self.gap_newlines += 1;
                        }
                    }
                    RSyntaxKind::SEMICOLON => {
                        self.gap_semicolons += 1;
                        if self.pending_semicolon.is_none() {
                            self.pending_semicolon = Some(token.range());
                        }
                    }
                    RSyntaxKind::COMMENT => self.trivia.handle_comment(self.text, token.range()),
                    _ => {
                        self.trivia.handle_token(self.text, token.range());
                        self.current = token;
                        return;
                    }
                },
            }
        }
    }

    /// Line break *or* semicolon before the current token: both terminate a
    /// statement in newline-significant contexts
    pub(crate) fn has_preceding_separator(&self) -> bool {
        self.gap_newlines > 0 || self.gap_semicolons > 0
    }

    /// Number of semicolons in the gap before the current token
    pub(crate) fn gap_semicolons(&self) -> u32 {
        self.gap_semicolons
    }

    /// Number of line breaks in the gap before the current token
    pub(crate) fn gap_newlines(&self) -> u32 {
        self.gap_newlines
    }

    /// Accept the semicolons in the gap before the current token as legal
    /// statement separators
    pub(crate) fn bless_semicolons(&mut self) {
        self.gap_semicolons = 0;
        self.pending_semicolon = None;
    }

    /// Record the current gap's still-pending semicolon as the first stray one,
    /// if we don't already have one
    fn finalize_pending_semicolon(&mut self) {
        if self.stray_semicolon.is_none() {
            self.stray_semicolon = self.pending_semicolon;
        }
    }

    /// If the current token is `]` and it is *immediately* followed by
    /// another `]` (no trivia in between), merge them into a single `]]`
    /// token, matching the single `]]` token tree-sitter lexes when closing a
    /// `[[`. Returns false otherwise.
    pub(crate) fn try_glue_right_bracket2(&mut self) -> bool {
        if self.current.kind() != RSyntaxKind::R_BRACK {
            return false;
        }
        let Some(next) = self.peek_lexer_token() else {
            return false;
        };
        if next.kind() != RSyntaxKind::R_BRACK || next.range().start() != self.current.range().end()
        {
            return false;
        }
        self.lookahead = None;
        self.current = Token::new(
            RSyntaxKind::R_BRACK2,
            TextRange::new(self.current.range().start(), next.range().end()),
        );
        // The merged token now ends one byte further right
        self.trivia.last_end = next.range().end();
        true
    }
}

impl TokenSource for RTokenSource<'_> {
    type Kind = RSyntaxKind;

    fn current(&self) -> RSyntaxKind {
        self.current.kind()
    }

    fn current_range(&self) -> TextRange {
        self.current.range()
    }

    fn text(&self) -> &str {
        self.text
    }

    fn has_preceding_line_break(&self) -> bool {
        self.gap_newlines > 0
    }

    fn bump(&mut self) {
        if self.current.kind() != RSyntaxKind::EOF {
            self.advance();
        }
    }

    fn skip_as_trivia(&mut self) {
        unreachable!("The parity parser never skips tokens as trivia");
    }

    fn finish(mut self) -> (Vec<Trivia>, Vec<ParseDiagnostic>) {
        // The final gap is never followed by an `advance`, so finalize it here.
        self.finalize_pending_semicolon();

        let mut diagnostics = Vec::new();
        if let Some((message, range)) = self.lexer.error() {
            diagnostics.push(ParseDiagnostic::new(message.clone(), *range));
        }
        if let Some(range) = self.stray_semicolon {
            diagnostics.push(ParseDiagnostic::new("Unexpected `;`.", range));
        }
        (self.trivia.pieces, diagnostics)
    }
}
