//! Loop invariant inference, after Flanagan and Leino's Houdini (FME 2001).
//!
//! A loop with no `@custom:invariant` gets candidate invariants built from what the program
//! already states: the range of its counter (from the initializer and the guard), the function's
//! postconditions with the loop bound replaced by the counter (what holds of the whole range at
//! exit holds of the part seen so far), its preconditions, and the guard's distance as a
//! termination measure. The verifier then keeps the greatest subset that is inductive: each round
//! checks every remaining candidate on entry and preserved by the body (assuming all of them at the
//! head) and drops those that fail, until none does. What survives is checked once more by the
//! ordinary loop obligations, so a candidate never adds trust: if the survivors do not prove the
//! function's specs, verification fails at that spec (fail closed).
//!
//! Candidates are only text. The substitution is token-level, so it can build a candidate that
//! means something unintended; such a candidate is either not inductive (dropped) or a true
//! invariant, and in both cases the proof stays sound.

/// The comparison a loop guard makes between its counter and its bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cmp {
    /// `i < n` (or `n > i`)
    Lt,
    /// `i <= n` (or `n >= i`)
    Le,
    /// `i != n`
    Ne,
}

/// What inference reads off a loop's header.
#[derive(Clone, Debug)]
pub(crate) struct Shape {
    /// The counter the guard compares, a plain variable.
    pub counter: String,
    /// The bound's source text.
    pub bound: String,
    pub cmp: Cmp,
    /// The counter's value before the first iteration (a `for` initializer), as source text.
    pub lower: Option<String>,
    /// The step increments the counter by one (`i++`, `++i`, `i += 1`, `i = i + 1`).
    pub step_up: bool,
}

/// Tag (`invariant` or `decreases`) and text of every candidate for a loop of shape `sh` in a
/// function with these preconditions and postconditions, without duplicates.
pub(crate) fn candidates(
    sh: Option<&Shape>,
    requires: &[&str],
    ensures: &[&str],
) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = vec![];
    let mut push = |tag: &'static str, t: String| {
        if !out.iter().any(|(g, x)| *g == tag && *x == t) {
            out.push((tag, t));
        }
    };
    if let Some(sh) = sh {
        let (i, b) = (&sh.counter, &sh.bound);
        if sh.step_up {
            if let Some(l) = &sh.lower {
                push("invariant", format!("{l} <= {i}"));
            }
            match sh.cmp {
                Cmp::Lt | Cmp::Ne => push("invariant", format!("{i} <= {b}")),
                Cmp::Le => push("invariant", format!("{i} <= ({b}) + 1")),
            }
        }
        // The bound generalized to the counter. With `i <= n` the part seen so far ends at
        // `i - 1`, not `i`.
        let upto = match sh.cmp {
            Cmp::Lt | Cmp::Ne => i.clone(),
            Cmp::Le => format!("({i} - 1)"),
        };
        for e in ensures {
            if let Some(t) = subst(e, b, &upto, i) {
                push("invariant", t);
            }
        }
        if sh.step_up {
            match sh.cmp {
                Cmp::Lt | Cmp::Ne => push("decreases", format!("({b}) - {i}")),
                Cmp::Le => push("decreases", format!("({b}) + 1 - {i}")),
            }
        }
    }
    for e in ensures.iter().chain(requires) {
        push("invariant", e.to_string());
    }
    out
}

/// Token spans of `s`: identifiers and numbers (`[A-Za-z0-9_$]+`) and single other characters,
/// without whitespace.
fn tokens(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$';
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
        } else if word(b[i]) {
            let j = i;
            while i < b.len() && word(b[i]) {
                i += 1;
            }
            out.push((j, i));
        } else {
            let n = s[i..].chars().next().map_or(1, char::len_utf8);
            out.push((i, i + n));
            i += n;
        }
    }
    out
}

/// `text` with every occurrence of the token sequence `from` replaced by `to`; None when there
/// is none, or when `text` already mentions `fresh` (the replacement could be captured by a
/// binder of that name, or confused with it).
pub(crate) fn subst(text: &str, from: &str, to: &str, fresh: &str) -> Option<String> {
    let tt = tokens(text);
    let ft: Vec<&str> = tokens(from).into_iter().map(|(a, b)| &from[a..b]).collect();
    if ft.is_empty() || tt.iter().any(|&(a, b)| &text[a..b] == fresh) {
        return None;
    }
    let mut out = String::new();
    let mut last = 0;
    let mut k = 0;
    let mut hit = false;
    while k < tt.len() {
        let m = k + ft.len() <= tt.len()
            && ft.iter().enumerate().all(|(n, f)| &text[tt[k + n].0..tt[k + n].1] == *f)
            // not a member of something else: `a.xs.length` is not `xs.length`
            && (k == 0 || &text[tt[k - 1].0..tt[k - 1].1] != ".");
        if m {
            out.push_str(&text[last..tt[k].0]);
            out.push_str(to);
            last = tt[k + ft.len() - 1].1;
            k += ft.len();
            hit = true;
        } else {
            k += 1;
        }
    }
    out.push_str(&text[last..]);
    hit.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_tokens() {
        let t = "forall uint256 j :: j < xs.length ==> xs[j] <= m";
        assert_eq!(
            subst(t, "xs.length", "i", "i").as_deref(),
            Some("forall uint256 j :: j < i ==> xs[j] <= m")
        );
        assert_eq!(subst("a.xs.length > 0", "xs.length", "i", "i"), None);
        assert_eq!(subst("n10 > n", "n", "k", "k").as_deref(), Some("n10 > k"));
        // the counter is already bound in the text: no candidate
        assert_eq!(subst("forall uint256 i :: i < n ==> a[i] > 0", "n", "i", "i"), None);
    }

    #[test]
    fn array_max_candidates() {
        let sh = Shape {
            counter: "i".into(),
            bound: "xs.length".into(),
            cmp: Cmp::Lt,
            lower: Some("1".into()),
            step_up: true,
        };
        let c = candidates(
            Some(&sh),
            &["xs.length > 0"],
            &[
                "forall uint256 j :: j < xs.length ==> xs[j] <= m",
                "exists uint256 j :: j < xs.length && xs[j] == m",
            ],
        );
        let inv: Vec<&str> =
            c.iter().filter(|(t, _)| *t == "invariant").map(|(_, x)| x.as_str()).collect();
        assert!(inv.contains(&"1 <= i"));
        assert!(inv.contains(&"i <= xs.length"));
        assert!(inv.contains(&"forall uint256 j :: j < i ==> xs[j] <= m"));
        assert!(inv.contains(&"exists uint256 j :: j < i && xs[j] == m"));
        assert!(c.contains(&("decreases", "(xs.length) - i".to_string())));
    }
}
