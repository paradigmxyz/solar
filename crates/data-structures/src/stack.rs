//! Stack growth for deeply recursive code.

/// The amount of stack that must remain before [`ensure_sufficient_stack`] switches to a new stack
/// segment.
///
/// It must cover the stack that any code between two calls to [`ensure_sufficient_stack`] uses.
const RED_ZONE: usize = 100 * 1024;

/// The size of each new stack segment.
const STACK_PER_RECURSION: usize = 1024 * 1024;

/// Runs `f`, first switching to a new stack segment if the current one is close to its end.
///
/// Call this at the entry of recursive functions whose depth depends on the input, such as
/// expression and statement visitors, so that deep nesting cannot overflow the stack. Each call
/// costs a stack pointer check, so avoid it in hot code that does not recurse.
#[inline]
pub fn ensure_sufficient_stack<R>(f: impl FnOnce() -> R) -> R {
    stacker::maybe_grow(RED_ZONE, STACK_PER_RECURSION, f)
}
