//! Shared pass timing output.

use std::{fmt, time::Instant};

pub(crate) struct PassTimer(Option<Instant>);

impl PassTimer {
    #[inline]
    pub(crate) fn new(enabled: bool) -> Self {
        Self(enabled.then(Instant::now))
    }

    pub(crate) fn finish(self, layer: &str, module: impl fmt::Display, pass: &str, changed: bool) {
        let Some(start) = self.0 else { return };
        eprintln!(
            "time: {:>7.3}\t{layer} {module} {pass} changed={changed}",
            start.elapsed().as_secs_f64()
        );
    }
}
