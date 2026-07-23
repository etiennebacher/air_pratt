//! Snapshot tests for the tricky corners of R parsing
//!
//! These snippets were originally differential-tested against the tree-sitter
//! backend before it was removed: every non-error tree and every error
//! verdict below was byte-identical to tree-sitter-r (pinned rev `d459b97`)
//! at the time of the swap, and the whole ~12k-file corpus of
//! `~/Desktop/Git` + `~/R` parsed identically. The snapshot locks in that
//! behavior: newline/semicolon significance, `]]` gluing, argument holes,
//! contextual keywords, optional extract/namespace operands, and failure
//! parity (any error collapses the file into one bogus expression).

use air_r_parser::RParserOptions;
use air_r_parser::parse;

const SNIPPETS: &[&str] = &[
    "",
    "\n\n",
    "# just a comment",
    "# comment, no trailing newline",
    "x",
    // Semicolons fold into whitespace trivia; stray ones are errors
    "1;2",
    "1 ; 2",
    "1;;2",
    ";x",
    "1;",
    "{ 1; }",
    "f(a;)",
    "f(a; b)",
    "(1;)",
    "x$;y",
    "pkg::;name",
    // `]]` gluing
    "x[[1]]",
    "x[y[1]]",
    "x[[1] ]",
    "x[[y]][2]",
    "a[b[[c]]]",
    "l[[c(1,2)]]",
    "x[[",
    "x[[a]",
    // Newline significance
    "1\n+2",
    "(1\n+2)",
    "{1\n+2}",
    "{(1\n+2)}",
    "f(1\n+2)",
    "x <-\n1",
    "x\n<- 1",
    "f\n(x)",
    "x$\ny",
    "x@\ny",
    // An extract selector survives any number of line breaks ; lone `\r` is
    // not a line break at all
    "x$\n\ny",
    "{\nx$\n\ny\n}",
    "(x$\n\n\ny)",
    "x$ # c\ny",
    "x$\n# c\ny",
    "x\r<- 1",
    "x$\r\ry",
    "x$\r\n\r\ny",
    "1 +\n\n2",
    "x <-\n\n1",
    "if (c)\n\nx",
    "A + ~B + C ~ D",
    "?x ? y",
    // `else` across line breaks: anywhere but top level
    "if (c) x else y",
    "if (c) x\nelse y",
    "{if (c) x\nelse y}",
    "(if (c) x\nelse y)",
    "if (c) x; else y",
    "if (c) if (d) x else y",
    "if (c) x = 1",
    "if\n(c)\nx",
    // Incomplete extract/namespace parses (missing operands)
    "x$",
    "x@",
    "pkg::",
    "x$ + 1",
    "f()::x",
    "x$y::z",
    "pkg:::\"name\"",
    "x$\"y\"",
    // Arguments: holes, names, optional values
    "f()",
    "f(,)",
    "f(,,)",
    "f(a,)",
    "f(,,a,,b,,)",
    "f(a b)",
    "f(x = )",
    "f(x = 1)",
    "f(\"x\" = 1)",
    "f(NULL = 1)",
    "f(... = 1)",
    "f(..1 = 1)",
    "f(a[1] = 2)",
    "x[, 1]",
    "x[i = 1]",
    "x[[i, j]]",
    // Functions and parameters
    "\\(x) x",
    "function() 1",
    "function(x, y = 1) x + y",
    "function(x,)",
    "function(...) 1",
    "function(x = c(1, 2)) x",
    // Precedence and associativity
    "-x^2",
    "-x*y",
    "a <- b <- c",
    "a -> b -> c",
    "a ^ b ^ c",
    "2--3",
    "!a == b",
    "~x + y",
    "?mean",
    "x ? y",
    "a:b:c",
    "x |> f() |> g()",
    "a %in% b %o% c",
    "function(x) x |> f()",
    "function(x) x ? y",
    "if (c) x <- 1",
    "while (c) x <- 1",
    "repeat x",
    "for (i in 1:10) print(i)",
    // Contextual keywords: `else`/`in` demote to identifiers where the
    // keyword can't attach; other keywords demote in identifier-only
    // positions (parameter names, `for` variables) but stay reserved
    // where a statement could start (`x$if` errors)
    "else <- 1",
    "in <- 1",
    "elsewhere",
    "else_idx <- 1",
    "x$else",
    "x$in",
    "x$TRUE",
    "x$if",
    "x$function",
    "pkg::else",
    "function(if) 1",
    "function(else) 1",
    "function(TRUE) 1",
    "for (function in x) 1",
    "f(else = 1)",
    "f(in = 1)",
    "f(TRUE = 1)",
    "if (c) x else\ny",
    // Numbers and identifiers
    "0x",
    "1e",
    "1e+",
    ".1L",
    ".1i",
    ".1foo",
    "..1",
    "..1a",
    "0x.p+1",
    "5L2",
    "1Li",
    "héllo <- 1",
    "`a b` <- 1",
    "r\"(a)b)\"",
    "R\"---[x]---\"",
    "r\"abc\"",
    "%no end",
    "\"no end",
    "`no end",
    // BOM: fills the root's bom slot (the old tree-sitter backend panicked)
    "\u{feff}x <- 1",
    "\u{feff}",
    // Miscellaneous errors
    "()",
    "x +",
    "else",
    "in",
    "{",
    "}",
    "f(",
    "a b",
    // Error recovery: statements around a syntax error still parse; the
    // failed statement becomes one localized R_BOGUS_EXPRESSION
    "one <- 1\nf(a b)\ntwo <- 2",
    "one <- 1; ) ; two <- 2",
    "{\n  good()\n  if broken\n  also_good()\n}",
    "f(a b) g(c d)\nfine()",
];

#[test]
fn test_snippets() {
    let mut snapshot = String::new();

    for snippet in SNIPPETS {
        let parsed = parse(snippet, RParserOptions::default());
        let error = match parsed.error() {
            Some(_) => " (parse error)",
            None => "",
        };
        snapshot.push_str(&format!(
            "{snippet:?}{error}\n{:#?}\n====================\n\n",
            parsed.syntax()
        ));
    }

    insta::assert_snapshot!(snapshot);
}
