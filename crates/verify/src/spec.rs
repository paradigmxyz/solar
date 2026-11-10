//! Spec-language preprocessing: Verus-style `forall|x| ...` / `==>` are not Solidity syntax, so
//! we rewrite them into calls Solar's expression parser accepts:
//!   `forall uint256 i, address a :: body`  ->  `__q0(body)` (binders recorded in a side table)
//!   `a ==> b`                              ->  `__implies((a), (b))`
//!   `a <==> b`                             ->  `((a) == (b))`, binding looser than `==>`

#[derive(Debug, Clone)]
pub(crate) struct QInfo {
    pub forall: bool,
    pub binders: Vec<(String, String)>,
}

pub(crate) fn preprocess(s: &str, quants: &mut Vec<QInfo>) -> Result<String, String> {
    let s = rewrite_quants(s, quants)?;
    Ok(rewrite_implies(&s))
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

fn rewrite_quants(s: &str, quants: &mut Vec<QInfo>) -> Result<String, String> {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let kw = ["forall", "exists"].into_iter().find(|k| {
            s[i..].starts_with(k)
                && (i == 0 || !is_ident(b[i - 1]))
                && b.get(i + k.len()).is_some_and(|c| !is_ident(*c))
        });
        if let Some(kw) = kw {
            let start = i + kw.len();
            let dc = s[start..]
                .find("::")
                .ok_or_else(|| format!("`{kw}` needs `::` before its body"))?
                + start;
            let binders_txt = s[start..dc].trim().trim_start_matches('|').trim_end_matches('|');
            let mut depth = 0i32;
            let mut j = dc + 2;
            while j < b.len() {
                match b[j] {
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1
                    }
                    _ => {}
                }
                j += 1;
            }
            let body = rewrite_quants(&s[dc + 2..j], quants)?;
            let mut binders = vec![];
            for part in binders_txt.split(',') {
                let part = part.trim();
                let (ty, name) = part
                    .rsplit_once(char::is_whitespace)
                    .ok_or_else(|| format!("binder `{part}` must be `<type> <name>`"))?;
                binders.push((ty.trim().to_string(), name.trim().to_string()));
            }
            let idx = quants.len();
            quants.push(QInfo { forall: kw == "forall", binders });
            out.push_str(&format!("__q{idx}({body})"));
            i = j;
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    Ok(out)
}

fn split_top<'s>(s: &'s str, sep: &str) -> Vec<&'s str> {
    let b = s.as_bytes();
    let mut parts = vec![];
    let mut depth = 0i32;
    let mut last = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 && s[i..].starts_with(sep) {
            parts.push(&s[last..i]);
            i += sep.len();
            last = i;
            continue;
        }
        i += 1;
    }
    parts.push(&s[last..]);
    parts
}

fn transform_groups(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'(' || b[i] == b'[' {
            let (open, close) = if b[i] == b'(' { (b'(', b')') } else { (b'[', b']') };
            let mut depth = 0;
            let mut k = i;
            while k < b.len() {
                if b[k] == open {
                    depth += 1
                } else if b[k] == close {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                k += 1;
            }
            out.push(open as char);
            out.push_str(&rewrite_implies(&s[i + 1..k.min(b.len())]));
            if k < b.len() {
                out.push(close as char);
            }
            i = k + 1;
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn rewrite_implies(s: &str) -> String {
    split_top(s, ",")
        .into_iter()
        .map(|part| {
            // `<==>` binds loosest: each side is a full spec expression, compared as a whole
            let sides = split_top(part, "<==>");
            if sides.len() > 1 {
                let parts: Vec<String> =
                    sides.iter().map(|x| rewrite_implies(x).trim().to_string()).collect();
                let mut acc = parts[0].clone();
                for p in &parts[1..] {
                    acc = format!("(({acc}) == ({p}))");
                }
                return acc;
            }
            let pieces: Vec<String> =
                split_top(part, "==>").into_iter().map(transform_groups).collect();
            let mut acc = pieces.last().unwrap().clone();
            for p in pieces[..pieces.len() - 1].iter().rev() {
                acc = format!("__implies(({}), ({}))", p.trim(), acc.trim());
            }
            acc
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rewrites() {
        let mut q = vec![];
        let s = preprocess("forall uint256 i :: i < n ==> a[i] <= m", &mut q).unwrap();
        assert_eq!(s, "__q0(__implies((i < n), (a[i] <= m)))");
        assert_eq!(q[0].binders, vec![("uint256".to_string(), "i".to_string())]);
        let s = preprocess("x > 0 && (y ==> z ==> w)", &mut q).unwrap();
        assert_eq!(s, "x > 0 && (__implies((y), (__implies((z), (w)))))");
        let s = preprocess("f(a ==> b, c)", &mut q).unwrap();
        assert_eq!(s, "f(__implies((a), (b)), c)");
    }
}
