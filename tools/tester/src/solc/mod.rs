use std::fmt;

pub(crate) mod solidity;
pub(crate) mod yul;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SolcError {
    pub kind: SolcErrorKind,
    pub code: Option<u32>,
    pub message: String,
}

impl fmt::Display for SolcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(code) = self.code {
            write!(f, " {code}")?;
        }
        write!(f, ": {}", self.message)
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

#[allow(dead_code)]
impl SolcErrorKind {
    pub fn is_parser_error(&self) -> bool {
        matches!(self, Self::DocstringParsingError | Self::ParserError)
    }

    pub(crate) fn is_error(&self) -> bool {
        !matches!(self, Self::Info | Self::Warning)
    }
}
