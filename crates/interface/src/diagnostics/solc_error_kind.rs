//! Solc Standard/JSON diagnostic `type` vocabulary.
//!
//! Matches the kinds solc emits in `errors[].type`, as modeled by the UI tester's
//! `SolcErrorKind` in `tools/tester/src/solc/mod.rs`.

use std::fmt;

/// Solc-compatible diagnostic kind for Standard/JSON `errors[].type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SolcErrorKind {
    /// Informational message.
    Info,
    /// Warning that did not stop compilation.
    Warning,
    /// Failure while generating code.
    CodeGenerationError,
    /// Invalid, unresolved, or clashing identifier.
    DeclarationError,
    /// NatSpec comment could not be parsed.
    DocstringParsingError,
    /// Source text does not conform to the language grammar.
    ParserError,
    /// Error within the type system.
    TypeError,
    /// Syntactical error, such as `continue` outside a loop.
    SyntaxError,
    /// IO or import processing failure.
    IOError,
    /// Fatal error not processed correctly.
    FatalError,
    /// Standard/JSON input does not conform to the required format.
    JSONError,
    /// Internal compiler bug.
    InternalCompilerError,
    /// Invalid use of the compiler stack.
    CompilerError,
    /// Unknown failure during compilation.
    Exception,
    /// Feature is not supported yet.
    UnimplementedFeatureError,
    /// Yul-specific exception.
    YulException,
    /// SMT checker exception.
    SMTLogicException,
}

impl SolcErrorKind {
    /// Returns the Standard/JSON `type` string for this kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Warning => "Warning",
            Self::CodeGenerationError => "CodeGenerationError",
            Self::DeclarationError => "DeclarationError",
            Self::DocstringParsingError => "DocstringParsingError",
            Self::ParserError => "ParserError",
            Self::TypeError => "TypeError",
            Self::SyntaxError => "SyntaxError",
            Self::IOError => "IOError",
            Self::FatalError => "FatalError",
            Self::JSONError => "JSONError",
            Self::InternalCompilerError => "InternalCompilerError",
            Self::CompilerError => "CompilerError",
            Self::Exception => "Exception",
            Self::UnimplementedFeatureError => "UnimplementedFeatureError",
            Self::YulException => "YulException",
            Self::SMTLogicException => "SMTLogicException",
        }
    }
}

impl fmt::Display for SolcErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SolcErrorKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Info" => Self::Info,
            "Warning" => Self::Warning,
            "CodeGenerationError" => Self::CodeGenerationError,
            "DeclarationError" => Self::DeclarationError,
            "DocstringParsingError" => Self::DocstringParsingError,
            "ParserError" => Self::ParserError,
            "TypeError" => Self::TypeError,
            "SyntaxError" => Self::SyntaxError,
            "IOError" => Self::IOError,
            "FatalError" => Self::FatalError,
            "JSONError" => Self::JSONError,
            "InternalCompilerError" => Self::InternalCompilerError,
            "CompilerError" => Self::CompilerError,
            "Exception" => Self::Exception,
            "UnimplementedFeatureError" => Self::UnimplementedFeatureError,
            "YulException" => Self::YulException,
            "SMTLogicException" => Self::SMTLogicException,
            _ => return Err(()),
        })
    }
}
