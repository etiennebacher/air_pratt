use air_r_syntax::RRoot;
use air_r_syntax::RSyntaxKind;
use air_r_syntax::RSyntaxNode;
use biome_parser::AnyParse;
use biome_parser::diagnostic::ParseDiagnostic;
use biome_parser::event::Event;
use biome_parser::prelude::Trivia;
use biome_rowan::AstNode;
use biome_rowan::NodeCache;
use biome_rowan::SendNode;

use crate::ParseError;
use crate::RLosslessTreeSink;
use crate::RParserOptions;

/// A utility struct for managing the result of a parser job
///
/// This struct holds a handle to the root node of the parsed syntax tree, along
/// with every diagnostic (each carrying a message and a [TextRange]) emitted by
/// the parser while generating this entry.
///
/// [TextRange]: biome_rowan::TextRange
///
/// It can be dynamically downcast into a concrete [RSyntaxNode] or [RRoot].
///
/// It can be sent or shared between threads.
///
/// This type is the same as [biome_parser::AnyParse], except it also offers a
/// [ParseError] view over the diagnostics, since [biome_parser::ParseDiagnostic]
/// oddly does not implement [std::error::Error] and we need that to compose with
/// other errors.
#[derive(Clone, Debug)]
pub struct Parse {
    root: SendNode,
    diagnostics: Vec<ParseDiagnostic>,
}

impl Parse {
    fn new(root: RSyntaxNode, diagnostics: Vec<ParseDiagnostic>) -> Parse {
        // Safety: This method is not exposed, we only use it internally
        let root = root.as_send().unwrap();
        Parse { root, diagnostics }
    }

    /// The syntax node represented by this Parse result
    pub fn syntax(&self) -> RSyntaxNode {
        self.root
            .clone()
            .into_node()
            .unwrap_or_else(|| panic!("Could not downcast root node to R language"))
    }

    /// Convert this parse result into a typed AST node
    pub fn tree(&self) -> RRoot {
        RRoot::unwrap_cast(self.syntax())
    }

    /// The diagnostics (message + range) the parser recorded, in the order they
    /// were produced
    pub fn diagnostics(&self) -> &[ParseDiagnostic] {
        &self.diagnostics
    }

    /// Convert this parse result into a [Result]
    pub fn into_result(self) -> Result<RSyntaxNode, ParseError> {
        match self.error() {
            Some(err) => Err(err),
            None => Ok(self.syntax()),
        }
    }

    /// Get the first error which occurred when parsing
    pub fn error(&self) -> Option<ParseError> {
        self.diagnostics
            .first()
            .map(|diagnostic| ParseError::new(diagnostic.message.to_string()))
    }

    /// Get the first error which occurred when parsing
    pub fn into_error(self) -> Option<ParseError> {
        self.error()
    }

    /// Returns [true] if the parser encountered some errors during the parsing.
    pub fn has_error(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

impl From<Parse> for AnyParse {
    fn from(parse: Parse) -> Self {
        Self::new(parse.root, parse.diagnostics)
    }
}

pub fn parse(text: &str, options: RParserOptions) -> Parse {
    let mut cache = NodeCache::default();
    let (events, tokens, diagnostics) = parse_text(text, options);
    build_tree(text, events, tokens, diagnostics, &mut cache)
}

fn build_tree(
    text: &str,
    events: Vec<Event<RSyntaxKind>>,
    tokens: Vec<Trivia>,
    diagnostics: Vec<ParseDiagnostic>,
    cache: &mut NodeCache,
) -> Parse {
    tracing::debug_span!("parse").in_scope(move || {
        // The tree sink has its own diagnostics channel (a holdover from
        // rust-analyzer). We've determined it does nothing here: whatever goes
        // in comes right back out. The real diagnostics travel alongside in
        // `diagnostics`.
        let sink_diagnostics = vec![];

        let mut tree_sink = RLosslessTreeSink::with_cache(text, &tokens, cache);
        biome_parser::event::process(&mut tree_sink, events, sink_diagnostics);
        let (green, _sink_diagnostics) = tree_sink.finish();

        Parse::new(green, diagnostics)
    })
}

fn parse_text(
    text: &str,
    _options: RParserOptions,
) -> (Vec<Event<RSyntaxKind>>, Vec<Trivia>, Vec<ParseDiagnostic>) {
    crate::grammar::parse_text(text)
}

#[cfg(test)]
mod tests {
    use biome_rowan::TextRange;
    use biome_rowan::TextSize;
    use biome_rowan::TriviaPieceKind;

    use super::*;

    enum Pos {
        Leading,
        Trailing,
    }

    fn trivia(text: &str) -> Vec<Trivia> {
        let (_events, trivia, _errors) = parse_text(text, RParserOptions::default());
        trivia
    }

    fn ws(start: u32, end: u32, position: Pos) -> Trivia {
        Trivia::new(
            TriviaPieceKind::Whitespace,
            TextRange::new(TextSize::from(start), TextSize::from(end)),
            matches!(position, Pos::Trailing),
        )
    }

    fn nl(start: u32, end: u32) -> Trivia {
        Trivia::new(
            TriviaPieceKind::Newline,
            TextRange::new(TextSize::from(start), TextSize::from(end)),
            false,
        )
    }

    fn cmt(start: u32, end: u32, position: Pos) -> Trivia {
        Trivia::new(
            TriviaPieceKind::SingleLineComment,
            TextRange::new(TextSize::from(start), TextSize::from(end)),
            matches!(position, Pos::Trailing),
        )
    }

    // TODO: It would be great if `biome_parser::token_source::Trivia`
    // implemented `PartialEq`, maybe we should ask for that.
    fn assert_eq_trivia(lhs: Vec<Trivia>, rhs: Vec<Trivia>) {
        assert_eq!(lhs.len(), rhs.len());

        for (i, (lhs, rhs)) in lhs.iter().zip(rhs.iter()).enumerate() {
            let message = format!("In event {i} with:\nlhs {lhs:?}\nrhs {rhs:?}");
            assert_eq!(lhs.kind(), rhs.kind(), "{message}");
            assert_eq!(lhs.text_range(), rhs.text_range(), "{message}");
            assert_eq!(lhs.trailing(), rhs.trailing(), "{message}");
        }
    }

    #[test]
    fn test_parse_trivia_smoke_test() {
        assert_eq_trivia(
            trivia("1 + 1"),
            vec![ws(1, 2, Pos::Leading), ws(3, 4, Pos::Leading)],
        );
    }

    #[test]
    fn test_parse_trivia_tab_test() {
        assert_eq_trivia(
            trivia("1\t+\t\n\t1"),
            vec![
                ws(1, 2, Pos::Leading),
                ws(3, 4, Pos::Trailing),
                nl(4, 5),
                ws(5, 6, Pos::Leading),
            ],
        );
    }

    #[test]
    fn test_parse_trivia_trailing_test() {
        assert_eq_trivia(
            trivia("1 + \n1"),
            vec![ws(1, 2, Pos::Leading), ws(3, 4, Pos::Trailing), nl(4, 5)],
        );
    }

    #[test]
    fn test_parse_trivia_trailing_trivia_test() {
        // Note that trivia between the last token and `EOF` is always
        // leading and will be attached to an `EOF` token by `TreeSink`.
        assert_eq_trivia(
            trivia("1  \n "),
            vec![ws(1, 3, Pos::Leading), nl(3, 4), ws(4, 5, Pos::Leading)],
        );
    }

    #[test]
    fn test_parse_trivia_trailing_crlf_test() {
        assert_eq_trivia(
            trivia("1 + \r\n1"),
            vec![ws(1, 2, Pos::Leading), ws(3, 4, Pos::Trailing), nl(4, 6)],
        );
    }

    #[test]
    fn test_parse_trivia_before_first_token() {
        assert_eq_trivia(trivia("  \n1"), vec![ws(0, 2, Pos::Leading), nl(2, 3)]);
    }

    #[test]
    fn test_parse_trivia_comment_test() {
        assert_eq_trivia(
            trivia("1 #"),
            vec![ws(1, 2, Pos::Trailing), cmt(2, 3, Pos::Trailing)],
        );
    }

    #[test]
    fn test_parse_trivia_comment_crlf_test() {
        // Trailing comment: `1 # hi\r\n` (r-lib/tree-sitter-r#184)
        assert_eq_trivia(
            trivia("1 # hi\r\n"),
            vec![ws(1, 2, Pos::Trailing), cmt(2, 6, Pos::Trailing), nl(6, 8)],
        );

        // Own-line comment: `# hi\r\n1`
        assert_eq_trivia(trivia("# hi\r\n1"), vec![cmt(0, 4, Pos::Leading), nl(4, 6)]);
    }

    #[test]
    fn test_parse_trivia_comment_nothing_else_test() {
        assert_eq_trivia(trivia("#"), vec![cmt(0, 1, Pos::Leading)]);
    }

    #[test]
    fn test_parse_trivia_comment_end_of_document_test() {
        assert_eq_trivia(trivia("1\n#"), vec![nl(1, 2), cmt(2, 3, Pos::Leading)]);
    }

    #[test]
    fn test_parse_trivia_whitespace_between_comments_test() {
        let text = "
1 #
#
2
"
        .trim();
        assert_eq_trivia(
            trivia(text),
            vec![
                ws(1, 2, Pos::Trailing),
                cmt(2, 3, Pos::Trailing),
                nl(3, 4),
                cmt(4, 5, Pos::Leading),
                nl(5, 6),
            ],
        );
    }

    #[test]
    fn test_parse_trivia_comment_beginning_of_document_test() {
        assert_eq_trivia(trivia("#\n1"), vec![cmt(0, 1, Pos::Leading), nl(1, 2)]);
    }

    #[test]
    fn test_parse_trivia_comment_beginning_of_document_with_whitespace_test() {
        assert_eq_trivia(
            trivia(" \n \n#"),
            vec![
                ws(0, 1, Pos::Leading),
                nl(1, 2),
                ws(2, 3, Pos::Leading),
                nl(3, 4),
                cmt(4, 5, Pos::Leading),
            ],
        );
    }

    /// The raw event streams were asserted here when the tree-sitter walker
    /// produced them; the Pratt parser builds an equivalent stream through
    /// `Marker::precede` (`forward_parent` links), so we lock in the
    /// resolved tree instead.
    #[test]
    fn test_parse_smoke_test() {
        let parsed = crate::parse("1+1", RParserOptions::default());
        insta::assert_snapshot!(format!("{:#?}", parsed.syntax()));
    }

    #[test]
    fn test_parse_function_definition() {
        let parsed = crate::parse("function() 1", RParserOptions::default());
        insta::assert_snapshot!(format!("{:#?}", parsed.syntax()));
    }

    #[test]
    fn test_parse_call() {
        let parsed = crate::parse("fn()", RParserOptions::default());
        insta::assert_snapshot!(format!("{:#?}", parsed.syntax()));
    }
}
