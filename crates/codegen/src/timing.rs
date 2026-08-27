//! Shared pass timing output.

use std::{
    fmt,
    time::{Duration, Instant},
};

/// Stable identity for one pipeline stage execution.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StageId {
    pub(crate) name: &'static str,
    pub(crate) round: usize,
}

impl StageId {
    pub(crate) const fn new(name: &'static str, round: usize) -> Self {
        Self { name, round }
    }
}

pub(crate) struct PassTimer(Option<Instant>);

pub(crate) struct StoppedPassTimer(Option<Duration>);

impl PassTimer {
    #[inline]
    pub(crate) fn new(enabled: bool) -> Self {
        Self(enabled.then(Instant::now))
    }

    pub(crate) fn stop(self) -> StoppedPassTimer {
        StoppedPassTimer(self.0.map(|start| start.elapsed()))
    }
}

impl StoppedPassTimer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish(
        self,
        layer: &str,
        module: impl fmt::Display,
        artifact: Option<&str>,
        pipeline_run: usize,
        stage: StageId,
        pass: &str,
        invocation: usize,
        ir_changed: bool,
        state_changed: bool,
        outcome: &str,
    ) {
        let Some(elapsed) = self.0 else { return };
        let elapsed = elapsed.as_secs_f64();
        let module = module.to_string();
        let artifact = artifact.unwrap_or("-");
        eprintln!(
            "time: {:>7.3}\tir={layer} module={module:?} artifact={artifact} \
             pipeline_run={pipeline_run} stage={} round={} pass={pass} invocation={invocation} \
             outcome={outcome} ir_changed={ir_changed} state_changed={state_changed} state_only={}",
            elapsed,
            stage.name,
            stage.round,
            state_changed && !ir_changed,
        );
    }
}
