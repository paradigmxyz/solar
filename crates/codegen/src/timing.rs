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
            "time: {:>10.3} ms\t{layer}\t{module}\t{pass}\tchanged={changed}",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
