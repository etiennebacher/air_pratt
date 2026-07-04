mod error;
mod grammar;
mod lexer;
mod options;
mod parse;
mod parser;
mod token_source;

use air_r_factory::RSyntaxFactory;
pub use error::ParseError;
pub use options::RParserOptions;
pub use parse::Parse;
pub use parse::parse;

use air_r_syntax::RLanguage;
use biome_parser::tree_sink::LosslessTreeSink;

pub(crate) type RLosslessTreeSink<'source> = LosslessTreeSink<'source, RLanguage, RSyntaxFactory>;
