//! Pathological inputs must parse without stack overflow
//!
//! The parser recurses per nesting level, so `grammar/expressions.rs` grows
//! the stack on demand (`stacker::maybe_grow`) at the two recursion funnels.
//! The old tree-sitter backend crashed outright on the flat 200k binary
//! chain below; these tests pin the new parser's robustness.
//!
//! Parsing is therefore safe at any depth. *Dropping* a very deep syntax tree
//! is not: biome_rowan recurses per level with no such stack growth — a
//! pre-existing limitation shared with the old backend that consumers hit long
//! after parsing succeeded, and one we can't fix from here. That recursive drop
//! is what actually overflows the stack, and 64 MiB was only ever enough to
//! absorb it on Linux/macOS: the MSVC build's larger per-level frames overflow
//! it on Windows.
//!
//! Since these tests only exercise the *parser*, we `mem::forget` the parse
//! result rather than drop it, so tree depth no longer has to fit the drop
//! recursion on any platform. Each case still runs on a thread with a large
//! fixed stack to cover parsing itself.

use air_r_parser::RParserOptions;
use air_r_parser::parse;

const STACK_SIZE: usize = 64 * 1024 * 1024;

fn assert_parses(name: &'static str, text: String, expect_error: bool) {
    std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(move || {
            let parsed = parse(&text, RParserOptions::default());
            assert_eq!(parsed.has_error(), expect_error, "case {name}");
            // Skip the recursive drop of the deep tree; the OS reclaims it when
            // the thread exits. See the module docs.
            std::mem::forget(parsed);
        })
        .unwrap()
        .join()
        .unwrap_or_else(|_| panic!("case {name} panicked"));
}

#[test]
fn test_deeply_nested_parens() {
    assert_parses(
        "parens",
        format!("{}x{}", "(".repeat(50000), ")".repeat(50000)),
        false,
    );
}

#[test]
fn test_deeply_nested_braces() {
    assert_parses(
        "braces",
        format!("{}x{}", "{".repeat(50000), "}".repeat(50000)),
        false,
    );
}

#[test]
fn test_long_binary_chain() {
    // Parsing handles far longer chains (the old backend aborted at 200k),
    // but the resulting left-nested tree must also fit the *drop* recursion
    // within STACK_SIZE, which caps this test around 100k
    let text = (0..100000)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("+");
    assert_parses("binary chain", text, false);
}

#[test]
fn test_long_unary_chain() {
    assert_parses("unary chain", format!("{}x", "!".repeat(100000)), false);
}

#[test]
fn test_long_right_assoc_chain() {
    let text = (0..100000).map(|_| "x").collect::<Vec<_>>().join(" <- ");
    assert_parses("right assoc chain", text, false);
}

#[test]
fn test_deep_call_nesting_without_parse_expression() {
    // Cycles through `parse_argument` -> `parse_expression_rest` without
    // entering `parse_expression`, which is why both funnels grow the stack
    assert_parses("alternating", "x[[y[".repeat(30000), true);
}

#[test]
fn test_flat_pathological_inputs() {
    assert_parses("dollars", format!("x{}", "$".repeat(100000)), false);
    assert_parses("semis", ";".repeat(200000), false);
    assert_parses("commas", format!("f({})", ",".repeat(100000)), false);
    assert_parses("unclosed brackets", "[[".repeat(100000), true);
    assert_parses("comments", "# c\n".repeat(200000), false);
    assert_parses("strings", "\"a\" ".repeat(100000), false);
}
