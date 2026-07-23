# R requires a selector after `$`/`@`. tree-sitter-r makes it `optional()`
# only so ark can complete at `x$<cursor>`; we match R and treat a missing
# selector as a parse error, recovering with an empty `right` slot. Anything
# that isn't an identifier, string, or dots selector (a number, keyword,
# parenthesized/braced expression, ...) leaves the selector missing.

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
