//! A hand-written lexer for R
//!
//! Token boundaries intentionally match what the tree-sitter-r grammar
//! produces, because the Pratt backend must build byte-identical trees to the
//! tree-sitter backend (see `grammar/mod.rs`). Notable consequences:
//!
//! - Strings are split into `STRING_OPEN` / `STRING_CONTENT?` / `STRING_CLOSE`
//!   tokens, and escape sequences are not sub-tokenized.
//! - `5L` and `5i` are single tokens (suffix glued onto the literal).
//! - `[[` is lexed greedily as `L_BRACK2`: two adjacent `[` can never validly
//!   mean anything else in R, since `[` cannot start an expression.
//! - `]` is always lexed as a single `R_BRACK`. In `x[y[1]]` the trailing `]]`
//!   closes two different subsets, so the decision of whether `]]` is one
//!   `R_BRACK2` token belongs to the parser, which glues two adjacent
//!   `R_BRACK` together when closing a `[[` (matching tree-sitter, which only
//!   lexes `]]` when a subset2 close is expected and the brackets are
//!   adjacent).

#[cfg(test)]
mod tests;

use air_r_syntax::RSyntaxKind;
use biome_rowan::TextRange;
use biome_rowan::TextSize;
use biome_unicode_table::is_js_id_continue;
use biome_unicode_table::is_js_id_start;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    kind: RSyntaxKind,
    range: TextRange,
}

impl Token {
    pub(crate) fn new(kind: RSyntaxKind, range: TextRange) -> Self {
        Self { kind, range }
    }

    pub(crate) fn kind(&self) -> RSyntaxKind {
        self.kind
    }

    pub(crate) fn range(&self) -> TextRange {
        self.range
    }
}

/// Lexer state for the interior of a string
///
/// A string is lexed as three tokens (`STRING_OPEN`, optional
/// `STRING_CONTENT`, `STRING_CLOSE`), so the lexer must remember that it is
/// inside a string between `next_token()` calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    InString {
        quote: u8,
    },
    InRawString {
        quote: u8,
        dashes: usize,
        /// The closing delimiter matching the `(`, `[`, or `{` of the open token
        close: u8,
    },
}

pub(crate) struct RLexer<'src> {
    text: &'src str,
    pos: usize,
    mode: Mode,
    /// First lexing error, if any, paired with the range of the offending
    /// token so callers can report a location (see `token_source.rs`).
    error: Option<(String, TextRange)>,
}

impl<'src> RLexer<'src> {
    pub(crate) fn new(text: &'src str) -> Self {
        Self {
            text,
            pos: 0,
            mode: Mode::Normal,
            error: None,
        }
    }

    pub(crate) fn error(&self) -> Option<&(String, TextRange)> {
        self.error.as_ref()
    }

    fn push_error(&mut self, message: impl Into<String>, range: TextRange) {
        if self.error.is_none() {
            self.error = Some((message.into(), range));
        }
    }

    /// The range of the token currently being lexed, `text[start..pos]`
    fn span(&self, start: usize) -> TextRange {
        TextRange::new(
            TextSize::try_from(start).unwrap(),
            TextSize::try_from(self.pos).unwrap(),
        )
    }

    fn bytes(&self) -> &'src [u8] {
        self.text.as_bytes()
    }

    fn byte_at(&self, offset: usize) -> Option<u8> {
        self.bytes().get(self.pos + offset).copied()
    }

    fn token(&mut self, kind: RSyntaxKind, start: usize) -> Token {
        let range = TextRange::new(
            TextSize::try_from(start).unwrap(),
            TextSize::try_from(self.pos).unwrap(),
        );
        Token { kind, range }
    }

    pub(crate) fn next_token(&mut self) -> Option<Token> {
        if self.pos >= self.text.len() {
            return None;
        }
        if self.pos == 0 && self.text.starts_with('\u{FEFF}') {
            self.pos = '\u{FEFF}'.len_utf8();
            return Some(self.token(RSyntaxKind::UNICODE_BOM, 0));
        }
        match self.mode {
            Mode::Normal => Some(self.lex_normal()),
            Mode::InString { quote } => Some(self.lex_in_string(quote)),
            Mode::InRawString {
                quote,
                dashes,
                close,
            } => Some(self.lex_in_raw_string(quote, dashes, close)),
        }
    }

    fn lex_normal(&mut self) -> Token {
        let start = self.pos;
        let byte = self.bytes()[self.pos];

        match byte {
            b'\n' => {
                self.pos += 1;
                self.token(RSyntaxKind::NEWLINE, start)
            }
            b'\r' => {
                self.pos += 1;
                if self.byte_at(0) == Some(b'\n') {
                    self.pos += 1;
                }
                self.token(RSyntaxKind::NEWLINE, start)
            }
            b' ' | b'\t' | 0x0B | 0x0C => {
                self.pos += 1;
                while matches!(self.byte_at(0), Some(b' ' | b'\t' | 0x0B | 0x0C)) {
                    self.pos += 1;
                }
                self.token(RSyntaxKind::WHITESPACE, start)
            }
            b';' => {
                self.pos += 1;
                self.token(RSyntaxKind::SEMICOLON, start)
            }
            b'#' => {
                while !matches!(self.byte_at(0), None | Some(b'\n' | b'\r')) {
                    self.pos += 1;
                }
                self.token(RSyntaxKind::COMMENT, start)
            }
            b'"' | b'\'' => {
                self.pos += 1;
                self.mode = Mode::InString { quote: byte };
                self.token(RSyntaxKind::STRING_OPEN, start)
            }
            b'`' => self.lex_quoted_identifier(),
            b'r' | b'R' if matches!(self.byte_at(1), Some(b'"' | b'\'')) => self.lex_raw_string_open(),
            b'0'..=b'9' => self.lex_number(),
            b'.' if matches!(self.byte_at(1), Some(b'0'..=b'9')) => self.lex_number(),
            b'(' => self.lex_single(RSyntaxKind::L_PAREN),
            b')' => self.lex_single(RSyntaxKind::R_PAREN),
            b'{' => self.lex_single(RSyntaxKind::L_CURLY),
            b'}' => self.lex_single(RSyntaxKind::R_CURLY),
            b'[' => {
                if self.byte_at(1) == Some(b'[') {
                    self.lex_multi(2, RSyntaxKind::L_BRACK2)
                } else {
                    self.lex_single(RSyntaxKind::L_BRACK)
                }
            }
            // Never `]]`, see module docs
            b']' => self.lex_single(RSyntaxKind::R_BRACK),
            b',' => self.lex_single(RSyntaxKind::COMMA),
            b'?' => self.lex_single(RSyntaxKind::WAT),
            b'~' => self.lex_single(RSyntaxKind::TILDE),
            b'$' => self.lex_single(RSyntaxKind::DOLLAR),
            b'@' => self.lex_single(RSyntaxKind::AT),
            b'^' => self.lex_single(RSyntaxKind::EXPONENTIATE),
            b'/' => self.lex_single(RSyntaxKind::DIVIDE),
            b'+' => self.lex_single(RSyntaxKind::PLUS),
            b'\\' => self.lex_single(RSyntaxKind::BACKSLASH),
            b'*' => {
                if self.byte_at(1) == Some(b'*') {
                    self.lex_multi(2, RSyntaxKind::EXPONENTIATE2)
                } else {
                    self.lex_single(RSyntaxKind::MULTIPLY)
                }
            }
            b'=' => {
                if self.byte_at(1) == Some(b'=') {
                    self.lex_multi(2, RSyntaxKind::EQUAL2)
                } else {
                    self.lex_single(RSyntaxKind::EQUAL)
                }
            }
            b'!' => {
                if self.byte_at(1) == Some(b'=') {
                    self.lex_multi(2, RSyntaxKind::NOT_EQUAL)
                } else {
                    self.lex_single(RSyntaxKind::BANG)
                }
            }
            b'&' => {
                if self.byte_at(1) == Some(b'&') {
                    self.lex_multi(2, RSyntaxKind::AND2)
                } else {
                    self.lex_single(RSyntaxKind::AND)
                }
            }
            b'|' => match self.byte_at(1) {
                Some(b'|') => self.lex_multi(2, RSyntaxKind::OR2),
                Some(b'>') => self.lex_multi(2, RSyntaxKind::PIPE),
                _ => self.lex_single(RSyntaxKind::OR),
            },
            b'<' => match (self.byte_at(1), self.byte_at(2)) {
                (Some(b'<'), Some(b'-')) => self.lex_multi(3, RSyntaxKind::SUPER_ASSIGN),
                (Some(b'-'), _) => self.lex_multi(2, RSyntaxKind::ASSIGN),
                (Some(b'='), _) => self.lex_multi(2, RSyntaxKind::LESS_THAN_OR_EQUAL_TO),
                _ => self.lex_single(RSyntaxKind::LESS_THAN),
            },
            b'>' => {
                if self.byte_at(1) == Some(b'=') {
                    self.lex_multi(2, RSyntaxKind::GREATER_THAN_OR_EQUAL_TO)
                } else {
                    self.lex_single(RSyntaxKind::GREATER_THAN)
                }
            }
            b'-' => match (self.byte_at(1), self.byte_at(2)) {
                (Some(b'>'), Some(b'>')) => self.lex_multi(3, RSyntaxKind::SUPER_ASSIGN_RIGHT),
                (Some(b'>'), _) => self.lex_multi(2, RSyntaxKind::ASSIGN_RIGHT),
                _ => self.lex_single(RSyntaxKind::MINUS),
            },
            b':' => match self.byte_at(1) {
                Some(b':') => {
                    if self.byte_at(2) == Some(b':') {
                        self.lex_multi(3, RSyntaxKind::COLON3)
                    } else {
                        self.lex_multi(2, RSyntaxKind::COLON2)
                    }
                }
                Some(b'=') => self.lex_multi(2, RSyntaxKind::WALRUS),
                _ => self.lex_single(RSyntaxKind::COLON),
            },
            b'%' => self.lex_special(),
            _ => {
                let char = self.current_char();
                if is_ident_start(char) {
                    self.lex_identifier_like()
                } else {
                    self.pos += char.len_utf8();
                    self.push_error(format!("Unexpected character `{char}`."), self.span(start));
                    self.token(RSyntaxKind::R_BOGUS, start)
                }
            }
        }
    }

    fn current_char(&self) -> char {
        self.text[self.pos..].chars().next().unwrap()
    }

    fn lex_single(&mut self, kind: RSyntaxKind) -> Token {
        self.lex_multi(1, kind)
    }

    fn lex_multi(&mut self, len: usize, kind: RSyntaxKind) -> Token {
        let start = self.pos;
        self.pos += len;
        self.token(kind, start)
    }

    /// `%[^%\\\n]*%`, matching tree-sitter-r's `special` token
    fn lex_special(&mut self) -> Token {
        let start = self.pos;
        self.pos += 1;
        loop {
            match self.byte_at(0) {
                Some(b'%') => {
                    self.pos += 1;
                    return self.token(RSyntaxKind::SPECIAL, start);
                }
                None | Some(b'\n' | b'\r' | b'\\') => {
                    self.push_error("Unterminated special operator.", self.span(start));
                    return self.token(RSyntaxKind::R_BOGUS, start);
                }
                Some(byte) => {
                    self.pos += if byte.is_ascii() {
                        1
                    } else {
                        self.current_char().len_utf8()
                    };
                }
            }
        }
    }

    /// Backtick-quoted identifier: `` `((?:\\(.|\n))|[^`\\])*` ``
    fn lex_quoted_identifier(&mut self) -> Token {
        let start = self.pos;
        self.pos += 1;
        loop {
            match self.byte_at(0) {
                Some(b'`') => {
                    self.pos += 1;
                    return self.token(RSyntaxKind::IDENT, start);
                }
                Some(b'\\') => {
                    // An escape consumes the following character, whatever it is
                    self.pos += 1;
                    match self.byte_at(0) {
                        Some(byte) if byte.is_ascii() => self.pos += 1,
                        Some(_) => self.pos += self.current_char().len_utf8(),
                        None => {
                            self.push_error("Unterminated quoted identifier.", self.span(start));
                            return self.token(RSyntaxKind::R_BOGUS, start);
                        }
                    }
                }
                Some(byte) => {
                    self.pos += if byte.is_ascii() {
                        1
                    } else {
                        self.current_char().len_utf8()
                    };
                }
                None => {
                    self.push_error("Unterminated quoted identifier.", self.span(start));
                    return self.token(RSyntaxKind::R_BOGUS, start);
                }
            }
        }
    }

    /// Open delimiter of a raw string: `[rR]['"]-*[([{]`
    ///
    /// Called with `pos` on the `r`/`R` and a quote in the next byte. If the
    /// dashes aren't followed by `(`, `[`, or `{`, this is not a raw string
    /// and the `r` is an ordinary identifier (matching tree-sitter, where the
    /// external raw string scanner fails and the parser falls back).
    fn lex_raw_string_open(&mut self) -> Token {
        let start = self.pos;
        let quote = self.bytes()[self.pos + 1];

        let mut offset = 2;
        while self.byte_at(offset) == Some(b'-') {
            offset += 1;
        }
        let close = match self.byte_at(offset) {
            Some(b'(') => b')',
            Some(b'[') => b']',
            Some(b'{') => b'}',
            _ => {
                // Not a raw string: lex just the `r` as an identifier
                self.pos += 1;
                return self.token(RSyntaxKind::IDENT, start);
            }
        };

        let dashes = offset - 2;
        self.pos += offset + 1;
        self.mode = Mode::InRawString {
            quote,
            dashes,
            close,
        };
        self.token(RSyntaxKind::STRING_OPEN, start)
    }

    fn lex_in_string(&mut self, quote: u8) -> Token {
        let start = self.pos;

        if self.byte_at(0) == Some(quote) {
            self.pos += 1;
            self.mode = Mode::Normal;
            return self.token(RSyntaxKind::STRING_CLOSE, start);
        }

        loop {
            match self.byte_at(0) {
                Some(byte) if byte == quote => {
                    return self.token(RSyntaxKind::STRING_CONTENT, start);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.byte_at(0) {
                        Some(byte) if byte.is_ascii() => self.pos += 1,
                        Some(_) => self.pos += self.current_char().len_utf8(),
                        None => break,
                    }
                }
                Some(byte) => {
                    self.pos += if byte.is_ascii() {
                        1
                    } else {
                        self.current_char().len_utf8()
                    };
                }
                None => break,
            }
        }

        self.push_error("Unterminated string.", self.span(start));
        self.mode = Mode::Normal;
        self.token(RSyntaxKind::R_BOGUS, start)
    }

    fn lex_in_raw_string(&mut self, quote: u8, dashes: usize, close: u8) -> Token {
        let start = self.pos;

        if self.at_raw_string_close(quote, dashes, close) {
            self.pos += 1 + dashes + 1;
            self.mode = Mode::Normal;
            return self.token(RSyntaxKind::STRING_CLOSE, start);
        }

        while self.byte_at(0).is_some() {
            if self.at_raw_string_close(quote, dashes, close) {
                return self.token(RSyntaxKind::STRING_CONTENT, start);
            }
            let byte = self.bytes()[self.pos];
            self.pos += if byte.is_ascii() {
                1
            } else {
                self.current_char().len_utf8()
            };
        }

        self.push_error("Unterminated raw string.", self.span(start));
        self.mode = Mode::Normal;
        self.token(RSyntaxKind::R_BOGUS, start)
    }

    fn at_raw_string_close(&self, quote: u8, dashes: usize, close: u8) -> bool {
        if self.byte_at(0) != Some(close) {
            return false;
        }
        for i in 0..dashes {
            if self.byte_at(1 + i) != Some(b'-') {
                return false;
            }
        }
        self.byte_at(1 + dashes) == Some(quote)
    }

    /// Numbers per `?NumericConstants`, with token boundaries matching
    /// tree-sitter-r:
    ///
    /// - hex: `0[xX](([0-9a-fA-F]+(\.[0-9a-fA-F]*)?)|(\.[0-9a-fA-F]*))([pP][+-]?[0-9]+)?`
    ///   (surprisingly, R allows `0x.`)
    /// - decimal: `(\d+(\.\d*)?|\.\d+)([eE][+-]?\d*)?` (note: empty exponent
    ///   digits are accepted by the grammar, so `1e` lexes as one number)
    /// - a trailing `L` makes it `R_INTEGER_LITERAL`, a trailing `i` makes it
    ///   `R_COMPLEX_LITERAL`, glued into a single token
    fn lex_number(&mut self) -> Token {
        let start = self.pos;

        let is_hex = self.byte_at(0) == Some(b'0')
            && matches!(self.byte_at(1), Some(b'x' | b'X'))
            && matches!(self.byte_at(2), Some(b) if b.is_ascii_hexdigit() || b == b'.');

        if is_hex {
            self.pos += 2;
            while matches!(self.byte_at(0), Some(b) if b.is_ascii_hexdigit()) {
                self.pos += 1;
            }
            if self.byte_at(0) == Some(b'.') {
                self.pos += 1;
                while matches!(self.byte_at(0), Some(b) if b.is_ascii_hexdigit()) {
                    self.pos += 1;
                }
            }
            // Binary exponent: [pP][+-]?[0-9]+ (digits required)
            if matches!(self.byte_at(0), Some(b'p' | b'P')) {
                let mut offset = 1;
                if matches!(self.byte_at(offset), Some(b'+' | b'-')) {
                    offset += 1;
                }
                if matches!(self.byte_at(offset), Some(b) if b.is_ascii_digit()) {
                    self.pos += offset;
                    while matches!(self.byte_at(0), Some(b) if b.is_ascii_digit()) {
                        self.pos += 1;
                    }
                }
            }
        } else {
            while matches!(self.byte_at(0), Some(b) if b.is_ascii_digit()) {
                self.pos += 1;
            }
            if self.byte_at(0) == Some(b'.') {
                self.pos += 1;
                while matches!(self.byte_at(0), Some(b) if b.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
            // Decimal exponent: [eE][+-]?\d* (empty digits allowed)
            if matches!(self.byte_at(0), Some(b'e' | b'E')) {
                self.pos += 1;
                if matches!(self.byte_at(0), Some(b'+' | b'-')) {
                    self.pos += 1;
                }
                while matches!(self.byte_at(0), Some(b) if b.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
        }

        // An `L`/`i` suffix only counts when nothing extends it into an
        // identifier: `5L` is an integer, but `5L2` is the double `5`
        // followed by the identifier `L2` (identifiers win the longest match)
        match self.byte_at(0) {
            Some(b'L') if !self.is_ident_continue_at(1) => {
                self.pos += 1;
                self.token(RSyntaxKind::R_INTEGER_LITERAL, start)
            }
            Some(b'i') if !self.is_ident_continue_at(1) => {
                self.pos += 1;
                self.token(RSyntaxKind::R_COMPLEX_LITERAL, start)
            }
            _ => self.token(RSyntaxKind::R_DOUBLE_LITERAL, start),
        }
    }

    /// Would the byte at `offset` continue an identifier (`[\p{XID_Continue}.]`)?
    fn is_ident_continue_at(&self, offset: usize) -> bool {
        match self.byte_at(offset) {
            None => false,
            Some(b'.') | Some(b'_') => true,
            Some(byte) if byte.is_ascii() => byte.is_ascii_alphanumeric(),
            Some(_) => {
                let char = self.text[self.pos + offset..].chars().next().unwrap();
                is_js_id_continue(char)
            }
        }
    }

    /// Unquoted identifier `[\p{XID_Start}._][\p{XID_Continue}.]*`, then
    /// classified into `DOTS` (`...`), `DOTDOTI` (`..1`), a keyword, or `IDENT`
    fn lex_identifier_like(&mut self) -> Token {
        let start = self.pos;

        loop {
            match self.byte_at(0) {
                Some(b'.') => self.pos += 1,
                Some(byte) if byte.is_ascii() => {
                    if byte.is_ascii_alphanumeric() || byte == b'_' {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                Some(_) => {
                    let char = self.current_char();
                    if is_js_id_continue(char) {
                        self.pos += char.len_utf8();
                    } else {
                        break;
                    }
                }
                None => break,
            }
        }

        let text = &self.text[start..self.pos];
        let kind = classify_identifier(text);
        self.token(kind, start)
    }
}

fn is_ident_start(char: char) -> bool {
    char == '.' || char == '_' || is_js_id_start(char)
}

fn classify_identifier(text: &str) -> RSyntaxKind {
    if text == "..." {
        return RSyntaxKind::DOTS;
    }
    if let Some(rest) = text.strip_prefix("..") {
        if !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()) {
            return RSyntaxKind::DOTDOTI;
        }
    }
    // `else` and `in` are *contextual*: they are never valid where an
    // expression starts, and tree-sitter's keyword extraction then falls back
    // to an identifier (`else <- 1` assigns to a variable named `else`). They
    // lex as IDENT and the parser remaps them to ELSE_KW/IN_KW in the two
    // positions where they act as keywords.
    match text {
        "function" => RSyntaxKind::FUNCTION_KW,
        "if" => RSyntaxKind::IF_KW,
        "for" => RSyntaxKind::FOR_KW,
        "while" => RSyntaxKind::WHILE_KW,
        "repeat" => RSyntaxKind::REPEAT_KW,
        "next" => RSyntaxKind::NEXT_KW,
        "break" => RSyntaxKind::BREAK_KW,
        "TRUE" => RSyntaxKind::TRUE_KW,
        "FALSE" => RSyntaxKind::FALSE_KW,
        "NULL" => RSyntaxKind::NULL_KW,
        "Inf" => RSyntaxKind::INF_KW,
        "NaN" => RSyntaxKind::NAN_KW,
        "NA" => RSyntaxKind::NA_LOGICAL_KW,
        "NA_integer_" => RSyntaxKind::NA_INTEGER_KW,
        "NA_real_" => RSyntaxKind::NA_DOUBLE_KW,
        "NA_complex_" => RSyntaxKind::NA_COMPLEX_KW,
        "NA_character_" => RSyntaxKind::NA_CHARACTER_KW,
        _ => RSyntaxKind::IDENT,
    }
}
