/// The short version information.
pub const SHORT_VERSION: &str = env!("SHORT_VERSION");

/// The long version information.
pub const LONG_VERSION: &str = concat!(
    env!("LONG_VERSION0"),
    "\n",
    env!("LONG_VERSION1"),
    "\n",
    env!("LONG_VERSION2"),
    "\n",
    env!("LONG_VERSION3"),
    "\n",
    env!("LONG_VERSION4"),
);
