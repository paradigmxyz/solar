use crate::map::FxHashSet;
use std::ops::ControlFlow;

// NOTE: `Cx` + `fn` emulates a closure. We can't use a closure directly because it would error with
// `closures cannot capture themselves or take themselves as argument`.

type F<Cx, T, B> = fn(Cx, &mut CycleDetector<Cx, T, B>, T) -> CycleDetectorResult<B, T>;

/// `?` for [`CycleDetectorResult`].
#[macro_export]
macro_rules! cdr_try {
    ($e:expr) => {
        match $e {
            CycleDetectorResult::Continue => {}
            e => return e,
        }
    };
}
pub use cdr_try;

/// Detector for cycles in directed graphs.
pub struct CycleDetector<Cx, T, B> {
    cx: Cx,
    first_cycle_vertex: Option<T>,
    depth: u32,
    processed: FxHashSet<T>,
    processing: FxHashSet<T>,
    f: F<Cx, T, B>,
}

/// Result of [`CycleDetector`].
pub enum CycleDetectorResult<B, T> {
    /// Processing continued.
    Continue,
    /// Processing stopped from a user-defined break.
    Break(B),
    /// A cycle was detected.
    Cycle(T),
}

impl<B, T> CycleDetectorResult<B, T> {
    /// Converts the result to a [`ControlFlow`].
    pub fn to_controlflow(self) -> ControlFlow<Self> {
        match self {
            Self::Continue => ControlFlow::Continue(()),
            _ => ControlFlow::Break(self),
        }
    }

    /// Returns `true` if the result is `Continue`.
    #[inline]
    pub fn is_continue(&self) -> bool {
        matches!(self, Self::Continue)
    }

    /// Returns `true` if the result is not `Continue`.
    #[inline]
    pub fn is_err(&self) -> bool {
        !self.is_continue()
    }

    /// Returns the value if the result is `Break`, otherwise `None`.
    #[inline]
    pub fn break_value(self) -> Option<B> {
        match self {
            Self::Break(b) => Some(b),
            _ => None,
        }
    }

    /// Returns the value if the result is `Cycle`, otherwise `None`.
    #[inline]
    pub fn cycle_value(self) -> Option<T> {
        match self {
            Self::Cycle(t) => Some(t),
            _ => None,
        }
    }
}

impl<Cx: Copy, T: Copy + Eq + std::hash::Hash, B> CycleDetector<Cx, T, B> {
    /// Creates a new cycle detector and runs it on the given vertex.
    ///
    /// Returns `Err` if a cycle is detected, containing the first vertex in the cycle.
    #[inline]
    pub fn detect(cx: Cx, vertex: T, f: F<Cx, T, B>) -> CycleDetectorResult<B, T> {
        Self::new(cx, f).run(vertex)
    }

    /// Creates a new cycle detector.
    #[inline]
    pub fn new(cx: Cx, f: F<Cx, T, B>) -> Self {
        Self {
            cx,
            first_cycle_vertex: None,
            depth: 0,
            processed: FxHashSet::default(),
            processing: FxHashSet::default(),
            f,
        }
    }

    /// Runs the cycle detector on the given vertex.
    ///
    /// Returns `Err` if a cycle is detected, containing the first vertex in the cycle.
    #[inline]
    pub fn run(&mut self, vertex: T) -> CycleDetectorResult<B, T> {
        cdr_try!(self.cycle_result());
        if self.processed.contains(&vertex) {
            return CycleDetectorResult::Continue;
        }
        if !self.processing.insert(vertex) {
            self.first_cycle_vertex = Some(vertex);
            return CycleDetectorResult::Cycle(vertex);
        }

        self.depth += 1;
        cdr_try!((self.f)(self.cx, self, vertex));
        self.depth -= 1;

        self.processing.remove(&vertex);
        self.processed.insert(vertex);

        self.cycle_result()
    }

    #[inline]
    fn cycle_result(&self) -> CycleDetectorResult<B, T> {
        match self.first_cycle_vertex {
            Some(vx) => CycleDetectorResult::Cycle(vx),
            None => CycleDetectorResult::Continue,
        }
    }

    /// Returns the current depth.
    #[inline]
    pub fn depth(&self) -> usize {
        self.depth as usize
    }
}
