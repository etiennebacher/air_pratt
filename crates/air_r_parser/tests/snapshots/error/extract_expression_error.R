# R requires a selector after `$`/`@`. A missing selector is a parse error,
# recovering with an empty `right` slot. Anything that isn't an identifier,
# string, or dots selector (a number, keyword, parenthesized/braced expression,
# ...) leaves the selector missing.

a$1
a$NA
a$NULL
a$TRUE
a$(b)
a${ b }

a@1
a@NA
a@NULL
a@TRUE
a@(b)
a@{ b }
