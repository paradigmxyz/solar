use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SolcError {
    pub kind: SolcErrorKind,
    pub code: u32,
    pub message: String,
}

impl fmt::Display for SolcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}: {}", self.kind, self.code, self.message)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SolcErrorKind {
    Info,
    Warning,
    CodeGenerationError,
    DeclarationError,
    DocstringParsingError,
    ParserError,
    TypeError,
    SyntaxError,
    IOError,
    FatalError,
    JSONError,
    InternalCompilerError,
    CompilerError,
    Exception,
    UnimplementedFeatureError,
    YulException,
    SMTLogicException,
}

impl fmt::Display for SolcErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
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

impl SolcErrorKind {
    pub fn parse_time_error(&self) -> bool {
        matches!(self, Self::DocstringParsingError | Self::ParserError | Self::SyntaxError)
    }
}
