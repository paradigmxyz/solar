//! Shared pass timing output.

use std::{
    fmt,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum PassOutcome {
    Ok,
    Failed,
    Skipped,
}

impl PassOutcome {
    pub(crate) const fn new(executed: bool, failed: bool) -> Self {
        if failed {
            Self::Failed
        } else if executed {
            Self::Ok
        } else {
            Self::Skipped
        }
    }

    pub(crate) const fn executed(self) -> bool {
        !matches!(self, Self::Skipped)
    }
}

impl fmt::Display for PassOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PassInfo<'a> {
    pub(crate) ir: &'static str,
    pub(crate) module: &'a str,
    pub(crate) artifact: Option<&'a str>,
    pub(crate) pipeline_run: usize,
    pub(crate) stage: &'static str,
    pub(crate) pass: &'static str,
    pub(crate) invocation: usize,
    pub(crate) outcome: PassOutcome,
    pub(crate) ir_changed: bool,
    pub(crate) state_changed: bool,
}

pub(crate) struct PassTimer(Instant);

pub(crate) struct StoppedPassTimer(Duration);

impl PassTimer {
    #[inline]
    pub(crate) fn new() -> Self {
        Self(Instant::now())
    }

    pub(crate) fn stop(self) -> StoppedPassTimer {
        StoppedPassTimer(self.0.elapsed())
    }
}

impl StoppedPassTimer {
    pub(crate) fn finish(self, info: PassInfo<'_>) {
        let elapsed = self.0.as_secs_f64();
        let artifact = info.artifact.unwrap_or("-");
        eprintln!(
            "time: {:>7.3}\tir={} module={:?} artifact={artifact} pipeline_run={} stage={} \
             pass={} invocation={} outcome={} ir_changed={} state_changed={} state_only={}",
            elapsed,
            info.ir,
            info.module,
            info.pipeline_run,
            info.stage,
            info.pass,
            info.invocation,
            info.outcome,
            info.ir_changed,
            info.state_changed,
            info.state_changed && !info.ir_changed,
        );
    }
}
