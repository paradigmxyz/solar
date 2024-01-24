xflags::xflags! {
    src "./src/flags.rs"

    /// Run custom build command.
    cmd xtask {
        /// Run tests.
        cmd test {
            /// Only run tests with the given name.
            optional test_name: String

            /// Bless test outputs.
            optional --bless

            repeated rest: String
        }
    }
}

// generated start
// The following code is generated by `xflags` macro.
// Run `env UPDATE_XFLAGS=1 cargo build` to regenerate.
#[derive(Debug)]
pub struct Xtask {
    pub subcommand: XtaskCmd,
}

#[derive(Debug)]
pub enum XtaskCmd {
    Test(Test),
}

#[derive(Debug)]
pub struct Test {
    pub test_name: Option<String>,
    pub rest: Vec<String>,

    pub bless: bool,
}

impl Xtask {
    #[allow(dead_code)]
    pub fn from_env_or_exit() -> Self {
        Self::from_env_or_exit_()
    }

    #[allow(dead_code)]
    pub fn from_env() -> xflags::Result<Self> {
        Self::from_env_()
    }

    #[allow(dead_code)]
    pub fn from_vec(args: Vec<std::ffi::OsString>) -> xflags::Result<Self> {
        Self::from_vec_(args)
    }
}
// generated end