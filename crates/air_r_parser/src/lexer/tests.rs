use air_r_syntax::RSyntaxKind;
use air_r_syntax::RSyntaxKind::*;

use crate::lexer::RLexer;

fn lex(text: &str) -> Vec<(RSyntaxKind, &str)> {
    let mut lexer = RLexer::new(text);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next_token() {
        tokens.push((token.kind(), &text[token.range()]));
    }
    assert_eq!(
        lexer.error(),
        None,
        "expected no lex errors for {text:?}, tokens: {tokens:?}"
    );
    tokens
}

fn lex_error(text: &str) -> Vec<(RSyntaxKind, &str)> {
    let mut lexer = RLexer::new(text);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next_token() {
        tokens.push((token.kind(), &text[token.range()]));
    }
    assert!(
        lexer.error().is_some(),
        "expected a lex error for {text:?}, tokens: {tokens:?}"
    );
    tokens
}

#[test]
fn test_smoke() {
    assert_eq!(
        lex("x <- 1"),
        vec![
            (IDENT, "x"),
            (WHITESPACE, " "),
            (ASSIGN, "<-"),
            (WHITESPACE, " "),
            (R_DOUBLE_LITERAL, "1"),
        ]
    );
}

#[test]
fn test_operators() {
    let cases: Vec<(&str, RSyntaxKind)> = vec![
        ("<-", ASSIGN),
        ("<<-", SUPER_ASSIGN),
        (":=", WALRUS),
        ("->", ASSIGN_RIGHT),
        ("->>", SUPER_ASSIGN_RIGHT),
        ("=", EQUAL),
        ("==", EQUAL2),
        ("!=", NOT_EQUAL),
        ("!", BANG),
        ("<", LESS_THAN),
        ("<=", LESS_THAN_OR_EQUAL_TO),
        (">", GREATER_THAN),
        (">=", GREATER_THAN_OR_EQUAL_TO),
        ("|", OR),
        ("||", OR2),
        ("|>", PIPE),
        ("&", AND),
        ("&&", AND2),
        ("+", PLUS),
        ("-", MINUS),
        ("*", MULTIPLY),
        ("**", EXPONENTIATE2),
        ("/", DIVIDE),
        ("^", EXPONENTIATE),
        (":", COLON),
        ("::", COLON2),
        (":::", COLON3),
        ("$", DOLLAR),
        ("@", AT),
        ("~", TILDE),
        ("?", WAT),
        ("\\", BACKSLASH),
        (",", COMMA),
        (";", SEMICOLON),
        ("(", L_PAREN),
        (")", R_PAREN),
        ("{", L_CURLY),
        ("}", R_CURLY),
        ("[", L_BRACK),
        ("]", R_BRACK),
        ("[[", L_BRACK2),
    ];
    for (text, kind) in cases {
        assert_eq!(lex(text), vec![(kind, text)], "lexing {text:?}");
    }
}

#[test]
fn test_close_brackets_never_glued() {
    // `]]` is two tokens; the parser glues them when closing a `[[`
    assert_eq!(lex("]]"), vec![(R_BRACK, "]"), (R_BRACK, "]")]);
}

#[test]
fn test_numbers() {
    let cases: Vec<(&str, RSyntaxKind)> = vec![
        ("1", R_DOUBLE_LITERAL),
        ("10.5", R_DOUBLE_LITERAL),
        ("5.", R_DOUBLE_LITERAL),
        (".5", R_DOUBLE_LITERAL),
        ("1e5", R_DOUBLE_LITERAL),
        ("1e+5", R_DOUBLE_LITERAL),
        ("1e-5", R_DOUBLE_LITERAL),
        ("1.5e2", R_DOUBLE_LITERAL),
        // tree-sitter-r allows empty exponent digits
        ("1e", R_DOUBLE_LITERAL),
        ("0x1F", R_DOUBLE_LITERAL),
        ("0xabcdef", R_DOUBLE_LITERAL),
        ("0x1p3", R_DOUBLE_LITERAL),
        ("0x1p-3", R_DOUBLE_LITERAL),
        // R allows fractional (and even empty) hex mantissas
        ("0x1.", R_DOUBLE_LITERAL),
        ("0x1.8", R_DOUBLE_LITERAL),
        ("0x.8", R_DOUBLE_LITERAL),
        ("0x.", R_DOUBLE_LITERAL),
        ("0x.8p2", R_DOUBLE_LITERAL),
        ("1L", R_INTEGER_LITERAL),
        ("0x1FL", R_INTEGER_LITERAL),
        ("1e5L", R_INTEGER_LITERAL),
        (".5L", R_INTEGER_LITERAL),
        ("1i", R_COMPLEX_LITERAL),
        ("1.5i", R_COMPLEX_LITERAL),
        (".5i", R_COMPLEX_LITERAL),
    ];
    for (text, kind) in cases {
        assert_eq!(lex(text), vec![(kind, text)], "lexing {text:?}");
    }
}

#[test]
fn test_number_boundaries() {
    // `0x` with no hex digits is the number `0` then the identifier `x`
    assert_eq!(lex("0x"), vec![(R_DOUBLE_LITERAL, "0"), (IDENT, "x")]);
    // Two dots can't be in one number
    assert_eq!(
        lex("1.2.3"),
        vec![(R_DOUBLE_LITERAL, "1.2"), (R_DOUBLE_LITERAL, ".3")]
    );
}

#[test]
fn test_identifiers() {
    let cases = vec!["foo", ".foo", "_foo", "foo.bar", "foo_bar", "a1", ".x2", "T", "F", "r", "R"];
    for text in cases {
        assert_eq!(lex(text), vec![(IDENT, text)], "lexing {text:?}");
    }
    // Unicode identifiers
    assert_eq!(lex("héllo"), vec![(IDENT, "héllo")]);
}

#[test]
fn test_quoted_identifiers() {
    assert_eq!(lex("`a b`"), vec![(IDENT, "`a b`")]);
    assert_eq!(lex("`a\\`b`"), vec![(IDENT, "`a\\`b`")]);
    assert_eq!(lex("`if`"), vec![(IDENT, "`if`")]);
}

#[test]
fn test_dots() {
    assert_eq!(lex("..."), vec![(DOTS, "...")]);
    assert_eq!(lex("..1"), vec![(DOTDOTI, "..1")]);
    assert_eq!(lex("..15"), vec![(DOTDOTI, "..15")]);
    // Not dot-dot-i: trailing alpha makes it a plain identifier
    assert_eq!(lex("..1a"), vec![(IDENT, "..1a")]);
    assert_eq!(lex("....."), vec![(IDENT, ".....")]);
    assert_eq!(lex("..."), vec![(DOTS, "...")]);
    assert_eq!(lex("."), vec![(IDENT, ".")]);
}

#[test]
fn test_keywords() {
    let cases: Vec<(&str, RSyntaxKind)> = vec![
        ("function", FUNCTION_KW),
        ("if", IF_KW),
        ("for", FOR_KW),
        ("while", WHILE_KW),
        ("repeat", REPEAT_KW),
        ("next", NEXT_KW),
        ("break", BREAK_KW),
        ("TRUE", TRUE_KW),
        ("FALSE", FALSE_KW),
        ("NULL", NULL_KW),
        ("Inf", INF_KW),
        ("NaN", NAN_KW),
        ("NA", NA_LOGICAL_KW),
        ("NA_integer_", NA_INTEGER_KW),
        ("NA_real_", NA_DOUBLE_KW),
        ("NA_complex_", NA_COMPLEX_KW),
        ("NA_character_", NA_CHARACTER_KW),
    ];
    for (text, kind) in cases {
        assert_eq!(lex(text), vec![(kind, text)], "lexing {text:?}");
    }
    // Keyword-looking prefixes are identifiers
    assert_eq!(lex("iffy"), vec![(IDENT, "iffy")]);
    assert_eq!(lex("TRUEISH"), vec![(IDENT, "TRUEISH")]);
    // `else` and `in` are contextual: the parser remaps them where they act
    // as keywords
    assert_eq!(lex("else"), vec![(IDENT, "else")]);
    assert_eq!(lex("in"), vec![(IDENT, "in")]);
}

#[test]
fn test_number_suffix_boundaries() {
    // A suffix only counts when nothing extends it into an identifier
    assert_eq!(lex("5L2"), vec![(R_DOUBLE_LITERAL, "5"), (IDENT, "L2")]);
    assert_eq!(lex("1Li"), vec![(R_DOUBLE_LITERAL, "1"), (IDENT, "Li")]);
    assert_eq!(lex("5L."), vec![(R_DOUBLE_LITERAL, "5"), (IDENT, "L.")]);
    assert_eq!(lex("1i2"), vec![(R_DOUBLE_LITERAL, "1"), (IDENT, "i2")]);
    assert_eq!(
        lex("1e5L2"),
        vec![(R_DOUBLE_LITERAL, "1e5"), (IDENT, "L2")]
    );
}

#[test]
fn test_strings() {
    assert_eq!(
        lex(r#""abc""#),
        vec![
            (STRING_OPEN, "\""),
            (STRING_CONTENT, "abc"),
            (STRING_CLOSE, "\""),
        ]
    );
    assert_eq!(
        lex("'abc'"),
        vec![
            (STRING_OPEN, "'"),
            (STRING_CONTENT, "abc"),
            (STRING_CLOSE, "'"),
        ]
    );
    // Empty string has no content token
    assert_eq!(lex(r#""""#), vec![(STRING_OPEN, "\""), (STRING_CLOSE, "\"")]);
    // Escaped quote stays inside the content
    assert_eq!(
        lex(r#""a\"b""#),
        vec![
            (STRING_OPEN, "\""),
            (STRING_CONTENT, r#"a\"b"#),
            (STRING_CLOSE, "\""),
        ]
    );
    // The other quote type is plain content
    assert_eq!(
        lex(r#""a'b""#),
        vec![
            (STRING_OPEN, "\""),
            (STRING_CONTENT, "a'b"),
            (STRING_CLOSE, "\""),
        ]
    );
    // Strings may span newlines
    assert_eq!(
        lex("\"a\nb\""),
        vec![
            (STRING_OPEN, "\""),
            (STRING_CONTENT, "a\nb"),
            (STRING_CLOSE, "\""),
        ]
    );
}

#[test]
fn test_raw_strings() {
    assert_eq!(
        lex(r#"r"(abc)""#),
        vec![
            (STRING_OPEN, "r\"("),
            (STRING_CONTENT, "abc"),
            (STRING_CLOSE, ")\""),
        ]
    );
    assert_eq!(
        lex(r#"R"---[abc]---""#),
        vec![
            (STRING_OPEN, "R\"---["),
            (STRING_CONTENT, "abc"),
            (STRING_CLOSE, "]---\""),
        ]
    );
    assert_eq!(
        lex("r'{a}b}'"),
        vec![
            (STRING_OPEN, "r'{"),
            (STRING_CONTENT, "a}b"),
            (STRING_CLOSE, "}'"),
        ]
    );
    // An unescaped quote inside raw content is fine
    assert_eq!(
        lex(r#"r"(a"b)""#),
        vec![
            (STRING_OPEN, "r\"("),
            (STRING_CONTENT, "a\"b"),
            (STRING_CLOSE, ")\""),
        ]
    );
    // Empty raw string
    assert_eq!(
        lex(r#"r"()""#),
        vec![(STRING_OPEN, "r\"("), (STRING_CLOSE, ")\"")]
    );
    // `r` not followed by a raw-string opener is an ordinary identifier
    assert_eq!(
        lex(r#"r"abc""#),
        vec![
            (IDENT, "r"),
            (STRING_OPEN, "\""),
            (STRING_CONTENT, "abc"),
            (STRING_CLOSE, "\""),
        ]
    );
}

#[test]
fn test_specials() {
    assert_eq!(lex("%in%"), vec![(SPECIAL, "%in%")]);
    assert_eq!(lex("%%"), vec![(SPECIAL, "%%")]);
    assert_eq!(lex("%o%"), vec![(SPECIAL, "%o%")]);
    assert_eq!(lex("%+ +%"), vec![(SPECIAL, "%+ +%")]);
}

#[test]
fn test_comments() {
    assert_eq!(
        lex("# hello\nx"),
        vec![(COMMENT, "# hello"), (NEWLINE, "\n"), (IDENT, "x")]
    );
    assert_eq!(lex("#"), vec![(COMMENT, "#")]);
}

#[test]
fn test_newlines() {
    assert_eq!(
        lex("a\nb\r\nc\rd"),
        vec![
            (IDENT, "a"),
            (NEWLINE, "\n"),
            (IDENT, "b"),
            (NEWLINE, "\r\n"),
            (IDENT, "c"),
            (NEWLINE, "\r"),
            (IDENT, "d"),
        ]
    );
}

#[test]
fn test_errors() {
    lex_error("\"abc"); // unterminated string
    lex_error("`abc"); // unterminated quoted identifier
    lex_error("%in"); // unterminated special
    lex_error("r\"(abc"); // unterminated raw string
}
