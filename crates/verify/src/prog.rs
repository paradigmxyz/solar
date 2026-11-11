//! Frontend: collect contracts, flatten inheritance and parse NatSpec specs with Solar's parser.
use crate::{
    spec::{QInfo, preprocess},
    ty::{Ty, parse_ty_text},
};
use solar_parse::{
    Parser,
    ast::{
        self, ContractKind, ElementaryType, Expr, ItemFunction, ItemKind, NatSpecKind, SourceUnit,
        Stmt, StmtKind, Type, TypeKind,
    },
    interface::{
        Session, Span,
        data_structures::map::{FxHashMap, FxHashSet},
        source_map::FileName,
    },
};

pub(crate) struct Spec<'a> {
    pub expr: &'a Expr<'a>,
    pub text: String,
    pub span: Span,
}

pub(crate) struct Func<'a> {
    pub name: String,
    pub contract: String,
    pub item: &'a ItemFunction<'a>,
    pub requires: Vec<Spec<'a>>,
    pub ensures: Vec<Spec<'a>>,
    /// `@custom:succeeds_if P`: from a pre-state where P (and the requires) hold, the function
    /// returns normally.
    pub succeeds: Vec<Spec<'a>>,
    pub params: Vec<(String, Ty)>,
    pub rets: Vec<(String, Ty)>,
    /// `@custom:spec` / pure-or-view single-expression body: usable inside specs.
    pub spec_body: Option<&'a Expr<'a>>,
    /// `@custom:check P`: a claim enforced at runtime (`--instrument` emits it as a require at
    /// the body's start) and then used as a fact; a failure blames the claim, not the caller.
    pub checks: Vec<Spec<'a>>,
    /// `@custom:opaque` on a spec function: specs see an uninterpreted function; its body is
    /// available only through `@custom:reveal f(args)`.
    pub opaque: bool,
    pub trusted: bool,
    pub synthetic: bool,
    /// `@custom:` statement specs in the body that no statement carries (a doc comment with no
    /// statement after it in its block): never silently dropped, verifying the function fails.
    pub orphans: Vec<(Span, String)>,
    /// Reached by `@custom:use` (a lemma, or a function a lemma runs): its ensures are assumed
    /// where it is used, so it must also return normally wherever its requires hold.
    pub ghost_fn: bool,
}

impl Func<'_> {
    pub(crate) fn has_contract(&self) -> bool {
        !self.requires.is_empty()
            || !self.ensures.is_empty()
            || !self.succeeds.is_empty()
            || self.item.body.is_none()
            || self.trusted
    }
    pub(crate) fn is_static(&self) -> bool {
        use ast::StateMutability::*;
        matches!(self.item.header.state_mutability(), Pure | View)
    }
    pub(crate) fn is_entry(&self) -> bool {
        use ast::Visibility::*;
        self.item.kind.is_constructor()
            || matches!(self.item.header.visibility(), None | Some(Public) | Some(External))
                && !self.item.kind.is_modifier()
    }
}

pub(crate) struct StateVar<'a> {
    pub name: String,
    pub ty: Ty,
    pub init: Option<&'a Expr<'a>>,
    pub constant: bool,
    pub immutable: bool,
    pub ghost: bool,
    /// The `@custom:ghost T name` tag that declared it: a user ghost variable, graded ghost by the
    /// non-interference check (the model's own ghosts, such as `__balance`, stand for real state).
    pub ghost_decl: Option<Span>,
    /// EIP-1153 transient storage (`transient` variables, and the ghost `__traw` behind
    /// tload/tstore): cleared at the end of every transaction.
    pub transient: bool,
}

pub(crate) struct Contract<'a> {
    pub name: String,
    pub kind: ContractKind,
    pub state: Vec<StateVar<'a>>,
    pub funcs: Vec<Func<'a>>,
    pub modifiers: FxHashMap<String, &'a ItemFunction<'a>>,
    pub invariants: Vec<Spec<'a>>,
    /// `@custom:lock-stable P`: entry points callable under the reentrancy lock keep P true.
    pub lock_stable: Vec<Spec<'a>>,
    pub lock: Option<(String, Span)>,
    /// `@custom:reentrancy-guard` given as a boolean expression (such as a transient slot):
    /// the lock holds when it is true.
    pub lock_expr: Option<Spec<'a>>,
    /// functions replaced by an override, kept for `super` calls
    pub overridden: Vec<Func<'a>>,
    /// linearization, bases first
    pub lin: Vec<String>,
    /// constructors of base contracts, in linearization order (bases first)
    pub base_ctors: Vec<(String, &'a ItemFunction<'a>)>,
    /// `is B(args)` arguments given anywhere in the hierarchy
    pub base_args: FxHashMap<String, &'a ast::CallArgs<'a>>,
}

pub(crate) struct Program<'a> {
    pub contracts: Vec<Contract<'a>>,
    pub quants: Vec<QInfo>,
    /// statement span -> (tag, spec) for `@custom:invariant|decreases|assert|ghost`
    pub stmt_specs: FxHashMap<Span, Vec<(String, Spec<'a>)>>,
    /// `@custom:assert` / `@custom:assume` comments after the last statement of a block (Solar
    /// attaches them to nothing), keyed by the block's span: they run when the block ends.
    pub block_tail: FxHashMap<Span, Vec<(String, Spec<'a>)>>,
    pub enums: FxHashMap<String, usize>,
    pub enum_variants: FxHashMap<String, usize>,
    pub structs: Vec<(String, Vec<(String, Ty)>)>,
    pub udvts: FxHashMap<String, Ty>,
    /// spans of variable-declaration statements that carry specs (--instrument keeps their scope)
    pub decl_stmts: FxHashSet<Span>,
    /// `@custom:invariant` on a struct declaration, over its field names
    pub struct_invs: FxHashMap<String, Vec<Spec<'a>>>,
    /// constants with a literal integer value (to size fixed arrays)
    pub const_sizes: FxHashMap<String, u64>,
    pub errors: Vec<(Span, String)>,
    /// Ghost code that is not erasable (see `crate::ghost`): a program with any is not verified.
    pub ghost_errors: Vec<crate::ghost::Violation>,
}

impl<'a> Program<'a> {
    pub(crate) fn struct_fields(&self, name: &str) -> Option<&Vec<(String, Ty)>> {
        self.structs.iter().find(|(n, _)| n == name).map(|(_, f)| f)
    }
    /// A library function callable as `x.f(args)` (`using L for T`), with `nargs` explicit args.
    /// Every library function callable as `x.f(args)`: overloads (such as OpenZeppelin
    /// TransientSlot's `tload` per slot type) are told apart by the caller.
    pub(crate) fn using_fns(&self, f: &str, nargs: usize) -> Vec<(&Contract<'a>, &Func<'a>)> {
        self.contracts
            .iter()
            .filter(|c| c.kind == ContractKind::Library)
            .flat_map(|c| {
                c.funcs
                    .iter()
                    .filter(move |g| g.name == f && g.params.len() == nargs + 1)
                    .map(move |g| (c, g))
            })
            .collect()
    }
    pub(crate) fn using_fn(&self, f: &str, nargs: usize) -> Option<(&Contract<'a>, &Func<'a>)> {
        self.contracts.iter().filter(|c| c.kind == ContractKind::Library).find_map(|c| {
            c.funcs.iter().find(|g| g.name == f && g.params.len() == nargs + 1).map(|g| (c, g))
        })
    }
    pub(crate) fn contract(&self, name: &str) -> Option<&Contract<'a>> {
        self.contracts.iter().find(|c| c.name == name)
    }
}

struct Builder<'s, 'a> {
    sess: &'s Session,
    arena: &'a ast::Arena,
    nspec: usize,
    quants: Vec<QInfo>,
    errors: Vec<(Span, String)>,
    enums: FxHashMap<String, usize>,
    enum_variants: FxHashMap<String, usize>,
    udvts: FxHashMap<String, &'a Type<'a>>,
    struct_items: Vec<&'a ast::ItemStruct<'a>>,
    struct_docs: Vec<(String, &'a ast::DocComments<'a>)>,
    decl_stmts: FxHashSet<Span>,
    contract_names: Vec<String>,
    stmt_specs: FxHashMap<Span, Vec<(String, Spec<'a>)>>,
    block_tail: FxHashMap<Span, Vec<(String, Spec<'a>)>>,
    const_sizes: FxHashMap<String, u64>,
    /// Spans of the doc comments some statement carries.
    seen_docs: Vec<Span>,
}

pub(crate) fn custom_tags(docs: &ast::DocComments<'_>) -> Vec<(String, String, Span)> {
    let mut v = vec![];
    for d in docs.iter() {
        for it in d.natspec.iter() {
            if let NatSpecKind::Custom { name } = it.kind {
                let content: String = it
                    .content()
                    .lines()
                    .map(|l| l.trim().trim_start_matches('*').trim())
                    .collect::<Vec<_>>()
                    .join(" ");
                v.push((name.as_str().to_string(), content.trim().to_string(), it.span));
            }
        }
    }
    v
}

impl<'s, 'a> Builder<'s, 'a> {
    fn parse_spec(&mut self, text: &str, span: Span) -> Option<Spec<'a>> {
        let pre = match preprocess(text, &mut self.quants) {
            Ok(p) => p,
            Err(e) => {
                self.errors.push((span, format!("bad spec `{text}`: {e}")));
                return None;
            }
        };
        self.nspec += 1;
        let mut p = Parser::from_source_code(
            self.sess,
            self.arena,
            FileName::Custom(format!("spec#{}", self.nspec)),
            pre,
        )
        .ok()?;
        match p.parse_expr() {
            Ok(_) if p.token.kind != solar_parse::token::TokenKind::Eof => {
                // never check a prefix of what was written: `a == b c` must not verify as `a == b`
                self.errors.push((
                    span,
                    format!("could not parse spec `{text}`: unexpected text after the expression"),
                ));
                None
            }
            Ok(e) => Some(Spec { expr: e, text: text.to_string(), span }),
            Err(d) => {
                d.emit();
                self.errors.push((span, format!("could not parse spec `{text}`")));
                None
            }
        }
    }

    pub(crate) fn ty(&mut self, t: &'a Type<'a>) -> Ty {
        match &t.kind {
            TypeKind::Elementary(e) => match e {
                ElementaryType::Bool => Ty::Bool,
                ElementaryType::Address(_) => Ty::Addr,
                ElementaryType::UInt(n) => Ty::Uint(n.bits()),
                ElementaryType::Int(n) => Ty::Int(n.bits()),
                ElementaryType::FixedBytes(n) => Ty::Uint(n.bytes() as u16 * 8),
                ElementaryType::String | ElementaryType::Bytes => Ty::Opaque,
            },
            TypeKind::Mapping(m) => Ty::Map(Box::new(self.ty(&m.key)), Box::new(self.ty(&m.value))),
            TypeKind::Array(a) => Ty::Arr(
                Box::new(self.ty(&a.element)),
                a.size.as_deref().and_then(|e| array_size(&self.const_sizes, e)),
            ),
            TypeKind::Function(_) => Ty::Opaque,
            TypeKind::Custom(p) => {
                let n = p.last().as_str().to_string();
                if let Some(k) = self.enums.get(&n) {
                    Ty::Enum(n, *k)
                } else if self.contract_names.contains(&n) {
                    Ty::Contract(n)
                } else if self.struct_items.iter().any(|st| st.name.as_str() == n) {
                    Ty::Struct(n)
                } else if let Some(u) = self.udvts.get(&n).copied() {
                    self.ty(u)
                } else {
                    self.errors.push((
                        t.span,
                        format!("unsupported type `{n}` (structs are not supported yet)"),
                    ));
                    Ty::Opaque
                }
            }
        }
    }

    /// `@custom:` doc comments inside `body` that no statement carries.
    fn orphan_specs(&self, body: Span) -> Vec<(Span, String)> {
        let Ok(text) = self.sess.source_map().span_to_snippet(body) else {
            return vec![];
        };
        let lo = body.lo();
        let mut out = vec![];
        let mut pos = 0usize;
        while let Some(i) = text[pos..].find("@custom:") {
            let at = pos + i;
            pos = at + 8;
            // only inside a doc comment: a `///` line, or a `/** */` block
            let line_start = text[..at].rfind('\n').map(|k| k + 1).unwrap_or(0);
            let line = &text[line_start..at];
            let in_line_doc = line.trim_start().starts_with("///");
            let in_block_doc = match (text[..at].rfind("/**"), text[..at].rfind("*/")) {
                (Some(o), Some(c)) => o > c,
                (Some(_), None) => true,
                _ => false,
            };
            if !in_line_doc && !in_block_doc {
                continue;
            }
            let abs = lo + solar_parse::interface::BytePos(at as u32);
            if self.seen_docs.iter().any(|d| d.lo() <= abs && abs < d.hi()) {
                continue;
            }
            let end = text[at..].find('\n').map(|k| at + k).unwrap_or(text.len());
            let sp = Span::new(abs, lo + solar_parse::interface::BytePos(end as u32));
            out.push((sp, text[at..end].trim().to_string()));
        }
        out
    }

    /// Attaches the orphan specs of `body` that end a block (`@custom:assert` / `assume` after
    /// its last statement) to that block; returns the rest, which are errors.
    fn attach_tails(&mut self, body: &'a ast::Block<'a>) -> Vec<(Span, String)> {
        let orphans = self.orphan_specs(body.span);
        if orphans.is_empty() {
            return orphans;
        }
        let mut blocks: Vec<&'a ast::Block<'a>> = vec![body];
        for st in body.stmts.iter() {
            crate::walk::all_stmts(st, &mut |x| match &x.kind {
                StmtKind::Block(b) | StmtKind::UncheckedBlock(b) => blocks.push(b),
                StmtKind::Try(t) => t.clauses.iter().for_each(|c| blocks.push(&c.block)),
                _ => {}
            });
        }
        let mut rest = vec![];
        for (sp, text) in orphans {
            let p = sp.lo();
            let inner = blocks
                .iter()
                .filter(|b| b.span.lo() <= p && p < b.span.hi())
                .min_by_key(|b| b.span.hi().0 - b.span.lo().0);
            let tail = inner.filter(|b| b.stmts.last().is_none_or(|l| l.span.hi() <= p));
            let body_txt = text.trim_start_matches("@custom:");
            let (tag, content) = body_txt.split_once(char::is_whitespace).unwrap_or((body_txt, ""));
            match tail {
                Some(b) if tag == "assert" || tag == "assume" => {
                    if let Some(spec) = self.parse_spec(content.trim(), sp) {
                        self.block_tail.entry(b.span).or_default().push((tag.to_string(), spec));
                    }
                }
                _ => rest.push((sp, text)),
            }
        }
        rest
    }

    fn walk_stmt_docs(&mut self, s: &'a Stmt<'a>) {
        for d in s.docs.iter() {
            self.seen_docs.push(d.span);
        }
        let tags = custom_tags(&s.docs);
        let mut v = vec![];
        for (tag, content, span) in tags {
            if ["invariant", "decreases", "assert", "ghost", "assume", "check", "use", "reveal"]
                .contains(&tag.as_str())
            {
                if tag == "check" && !executable(&content) {
                    self.errors.push((span, format!("@custom:check must be executable Solidity (no old(), quantifiers, ==> or <==>): it becomes a require: `{content}`")));
                    continue;
                }
                if let Some(sp) = self.parse_spec(&content, span) {
                    v.push((tag, sp));
                }
            }
        }
        if !v.is_empty() {
            if matches!(s.kind, StmtKind::DeclSingle(_) | StmtKind::DeclMulti(..)) {
                self.decl_stmts.insert(s.span);
            }
            self.stmt_specs.insert(s.span, v);
        }
        crate::walk::sub_stmts(s, &mut |c| self.walk_stmt_docs(c));
    }

    fn func(&mut self, cname: &str, item: &'a ast::Item<'a>, f: &'a ItemFunction<'a>) -> Func<'a> {
        let mut requires = vec![];
        let mut ensures = vec![];
        let mut succeeds = vec![];
        let mut trusted = false;
        let mut is_spec = false;
        let mut checks = vec![];
        let mut opaque = false;
        for (tag, content, span) in custom_tags(&item.docs) {
            match tag.as_str() {
                "requires" => requires.extend(self.parse_spec(&content, span)),
                "ensures" => ensures.extend(self.parse_spec(&content, span)),
                "succeeds_if" | "succeeds-if" => succeeds.extend(self.parse_spec(&content, span)),
                "trusted" | "external_body" => trusted = true,
                "check" => {
                    if executable(&content) {
                        checks.extend(self.parse_spec(&content, span))
                    } else {
                        self.errors.push((span, format!("@custom:check must be executable Solidity (no old(), quantifiers, ==> or <==>): it becomes a require: `{content}`")));
                    }
                }
                "opaque" => opaque = true,
                "spec" => is_spec = true,
                _ => {}
            }
        }
        if let Some(spec) = builtin_summary(cname, f) {
            // a library whose behavior is documented but whose 512-bit assembly is out of reach:
            // trusted at that documented contract (listed in the README's trusted assumptions)
            trusted = true;
            ensures.extend(self.parse_spec(spec, item.span));
        }
        let params = f
            .header
            .parameters
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    p.name.map(|n| n.as_str().to_string()).unwrap_or(format!("__arg{i}")),
                    self.ty(&p.ty),
                )
            })
            .collect();
        let rets = f
            .header
            .returns()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    p.name.map(|n| n.as_str().to_string()).unwrap_or(format!("__ret{i}")),
                    self.ty(&p.ty),
                )
            })
            .collect();
        let mut spec_body = None;
        let mut orphans = vec![];
        if let Some(b) = &f.body {
            for s in b.stmts.iter() {
                self.walk_stmt_docs(s);
            }
            orphans = self.attach_tails(b);
            use ast::StateMutability::*;
            if (is_spec || matches!(f.header.state_mutability(), Pure | View))
                && let [s] = &b.stmts[..]
                && let StmtKind::Return(Some(e)) = &s.kind
            {
                spec_body = Some(&**e);
            }
        }
        let name = match f.kind {
            ast::FunctionKind::Constructor => "constructor".to_string(),
            ast::FunctionKind::Fallback => "fallback".to_string(),
            ast::FunctionKind::Receive => "receive".to_string(),
            _ => f.header.name.map(|n| n.as_str().to_string()).unwrap_or_default(),
        };
        Func {
            name,
            contract: cname.to_string(),
            item: f,
            requires,
            ensures,
            succeeds,
            params,
            rets,
            spec_body,
            checks,
            opaque,
            trusted,
            synthetic: false,
            orphans,
            ghost_fn: false,
        }
    }
}

pub(crate) fn build<'a>(
    sess: &Session,
    arena: &'a ast::Arena,
    unit: &'a SourceUnit<'a>,
) -> Program<'a> {
    let mut b = Builder {
        sess,
        arena,
        nspec: 0,
        quants: vec![],
        errors: vec![],
        enums: FxHashMap::default(),
        enum_variants: FxHashMap::default(),
        udvts: FxHashMap::default(),
        struct_items: vec![],
        struct_docs: vec![],
        decl_stmts: Default::default(),
        contract_names: vec![],
        stmt_specs: FxHashMap::default(),
        block_tail: FxHashMap::default(),
        const_sizes: FxHashMap::default(),
        seen_docs: vec![],
    };
    let mut items: FxHashMap<String, (&'a ast::Item<'a>, &'a ast::ItemContract<'a>)> =
        FxHashMap::default();
    // file-level constants, visible in every contract
    let mut file_consts: Vec<(String, &'a ast::VariableDefinition<'a>)> = vec![];
    // inline assembly anywhere in a library (inlined into contracts that call it)
    let mut lib_asm = false;
    let mut order = vec![];
    let collect_names = |b: &mut Builder<'_, 'a>, it: &'a ast::Item<'a>| match &it.kind {
        ItemKind::Enum(e) => {
            b.enums.insert(e.name.as_str().to_string(), e.variants.len());
            for (i, v) in e.variants.iter().enumerate() {
                b.enum_variants.insert(format!("{}.{}", e.name.as_str(), v.as_str()), i);
            }
        }
        ItemKind::Udvt(u) => {
            b.udvts.insert(u.name.as_str().to_string(), &u.ty);
        }
        ItemKind::Struct(st) => {
            b.struct_items.push(st);
            b.struct_docs.push((st.name.as_str().to_string(), &it.docs));
        }
        ItemKind::Variable(v) => {
            if let (Some(n), Some(init)) = (v.name, v.initializer.as_deref())
                && let Some(k) = array_size(&b.const_sizes, init)
            {
                b.const_sizes.insert(n.as_str().to_string(), k);
            }
        }
        _ => {}
    };
    for it in unit.items.iter() {
        collect_names(&mut b, it);
        if let ItemKind::Variable(v) = &it.kind
            && let Some(n) = v.name
        {
            file_consts.push((n.as_str().to_string(), v));
        }
        if let ItemKind::Contract(c) = &it.kind
            && c.kind == ContractKind::Library
        {
            for sub in c.body.iter() {
                if let ItemKind::Function(f) = &sub.kind {
                    for st in f.body.iter().flat_map(|b| b.stmts.iter()) {
                        crate::walk::all_stmts(st, &mut |x| {
                            lib_asm |= matches!(x.kind, StmtKind::Assembly(_))
                        });
                    }
                }
            }
        }
        if let ItemKind::Contract(c) = &it.kind {
            let n = c.name.as_str().to_string();
            b.contract_names.push(n.clone());
            items.insert(n.clone(), (it, c));
            order.push(n);
            for sub in c.body.iter() {
                collect_names(&mut b, sub);
            }
        }
    }
    let mut structs = vec![];
    for st in b.struct_items.clone() {
        let fields = st
            .fields
            .iter()
            .map(|f| (f.name.map(|n| n.as_str().to_string()).unwrap_or_default(), b.ty(&f.ty)))
            .collect();
        structs.push((st.name.as_str().to_string(), fields));
    }
    let mut struct_invs: FxHashMap<String, Vec<Spec<'a>>> = FxHashMap::default();
    for (n, docs) in b.struct_docs.clone() {
        for (tag, content, span) in custom_tags(docs) {
            if tag == "invariant"
                && let Some(sp) = b.parse_spec(&content, span)
            {
                struct_invs.entry(n.clone()).or_default().push(sp);
            }
        }
    }
    let mut udvts = FxHashMap::default();
    for (n, t) in b.udvts.clone() {
        udvts.insert(n, b.ty(t));
    }
    let mut contracts = vec![];
    for name in &order {
        // linearize bases (simple DFS, bases first)
        let mut lin: Vec<String> = vec![];
        fn dfs<'a>(
            n: &str,
            items: &FxHashMap<String, (&'a ast::Item<'a>, &'a ast::ItemContract<'a>)>,
            lin: &mut Vec<String>,
        ) {
            if lin.iter().any(|x| x == n) {
                return;
            }
            if let Some((_, c)) = items.get(n) {
                for base in c.bases.iter() {
                    dfs(base.name.last().as_str(), items, lin);
                }
                lin.push(n.to_string());
            }
        }
        dfs(name, &items, &mut lin);
        let (_, own) = items[name];
        let mut con = Contract {
            name: name.clone(),
            kind: own.kind,
            state: vec![],
            funcs: vec![],
            modifiers: FxHashMap::default(),
            invariants: vec![],
            lock_stable: vec![],
            lock: None,
            lock_expr: None,
            overridden: vec![],
            lin: lin.clone(),
            base_ctors: vec![],
            base_args: FxHashMap::default(),
        };
        let mut has_asm = false;
        for cn in &lin {
            for sub in items[cn].1.body.iter() {
                if let ItemKind::Function(f) = &sub.kind {
                    for st in f.body.iter().flat_map(|b| b.stmts.iter()) {
                        crate::walk::all_stmts(st, &mut |x| {
                            has_asm |= matches!(x.kind, StmtKind::Assembly(_))
                        });
                    }
                }
            }
        }
        for cn in &lin {
            let (item, c) = items[cn];
            for (tag, content, span) in custom_tags(&item.docs) {
                match tag.as_str() {
                    "invariant" => con.invariants.extend(b.parse_spec(&content, span)),
                    "lock-stable" | "lock_stable" => {
                        con.lock_stable.extend(b.parse_spec(&content, span))
                    }
                    "reentrancy-guard" | "reentrancy_guard" => {
                        let t = content.trim().to_string();
                        let simple =
                            !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_');
                        con.lock_expr = if simple { None } else { b.parse_spec(&t, span) };
                        con.lock = Some((t, span))
                    }
                    "ghost" => {
                        let content = content.trim().trim_end_matches(';');
                        match content
                            .rsplit_once(char::is_whitespace)
                            .and_then(|(t, n)| Some((parse_ty_text(t)?, n)))
                        {
                            Some((ty, n)) => con.state.push(StateVar {
                                name: n.to_string(),
                                ty,
                                init: None,
                                constant: false,
                                immutable: false,
                                ghost: true,
                                ghost_decl: Some(span),
                                transient: false,
                            }),
                            None => {
                                b.errors.push((span, format!("bad ghost declaration `{content}`")))
                            }
                        }
                    }
                    _ => {}
                }
            }
            for base in c.bases.iter() {
                if !base.arguments.is_empty() {
                    con.base_args.insert(base.name.last().as_str().to_string(), &base.arguments);
                }
            }
            for sub in c.body.iter() {
                match &sub.kind {
                    ItemKind::Variable(v) => {
                        let ty = b.ty(&v.ty);
                        con.state.push(StateVar {
                            name: v.name.map(|n| n.as_str().to_string()).unwrap_or_default(),
                            ty,
                            init: v.initializer.as_deref(),
                            constant: v.mutability == Some(ast::VarMut::Constant),
                            immutable: v.mutability == Some(ast::VarMut::Immutable),
                            ghost: false,
                            ghost_decl: None,
                            transient: v.data_location == Some(ast::DataLocation::Transient),
                        });
                    }
                    ItemKind::Function(f) if f.kind.is_modifier() => {
                        if let Some(b2) = &f.body {
                            for s in b2.stmts.iter() {
                                b.walk_stmt_docs(s);
                            }
                            for (sp, t) in b.attach_tails(b2) {
                                b.errors.push((sp, format!("spec comment `{t}` is attached to no statement: only @custom:assert and @custom:assume may end a block; put a statement after it")));
                            }
                        }
                        con.modifiers.insert(
                            f.header.name.map(|n| n.as_str().to_string()).unwrap_or_default(),
                            f,
                        );
                    }
                    ItemKind::Function(f) => {
                        let func = b.func(cn, sub, f);
                        if let Some(i) = con.funcs.iter().position(|g| {
                            // an override comes from a base contract; one contract's own
                            // overloads (TransientSlot's tload per slot type) all stay, even
                            // when their parameter types erase to the same Ty
                            g.name == func.name
                                && g.contract != func.contract
                                && g.params
                                    .iter()
                                    .map(|p| &p.1)
                                    .eq(func.params.iter().map(|p| &p.1))
                                && func.name != "constructor"
                        }) {
                            let old = std::mem::replace(&mut con.funcs[i], func);
                            con.overridden.push(old);
                        } else if func.name == "constructor" && cn != name {
                            con.base_ctors.push((cn.clone(), f));
                        } else {
                            con.funcs.push(func);
                        }
                    }
                    _ => {}
                }
            }
        }
        for (n, v) in &file_consts {
            if v.mutability == Some(ast::VarMut::Constant) {
                let ty = b.ty(&v.ty);
                con.state.push(StateVar {
                    name: n.clone(),
                    ty,
                    init: v.initializer.as_deref(),
                    constant: true,
                    immutable: false,
                    ghost: false,
                    ghost_decl: None,
                    transient: false,
                });
            }
        }
        if (con.kind == ContractKind::Contract || con.kind == ContractKind::AbstractContract)
            && (has_asm || lib_asm)
        {
            // transient storage (tload/tstore): arbitrary when a call starts (it may reenter
            // mid-transaction), kept within it, havocked at external calls like storage
            con.state.push(StateVar {
                name: "__traw".into(),
                ty: Ty::Map(Box::new(Ty::Uint(256)), Box::new(Ty::Uint(256))),
                init: None,
                constant: false,
                immutable: false,
                ghost: true,
                ghost_decl: None,
                transient: true,
            });
            // raw storage slots that inline assembly reaches with sload/sstore (assumed disjoint
            // from the declared state variables)
            con.state.push(StateVar {
                name: "__raw".into(),
                ty: Ty::Map(Box::new(Ty::Uint(256)), Box::new(Ty::Uint(256))),
                init: None,
                constant: false,
                immutable: false,
                ghost: true,
                ghost_decl: None,
                transient: false,
            });
        }
        let uses_time = con.invariants.iter().any(|s| s.text.contains("__time"))
            || con
                .funcs
                .iter()
                .any(|f| f.requires.iter().chain(&f.ensures).any(|s| s.text.contains("__time")));
        if (con.kind == ContractKind::Contract || con.kind == ContractKind::AbstractContract)
            && uses_time
        {
            // the latest block.timestamp this contract has seen: block time never goes backwards,
            // so every call starts at or after it (added only when a spec mentions it)
            con.state.push(StateVar {
                name: "__time".into(),
                ty: Ty::Uint(256),
                init: None,
                constant: false,
                immutable: false,
                ghost: true,
                ghost_decl: None,
                transient: false,
            });
        }
        if con.kind == ContractKind::Contract || con.kind == ContractKind::AbstractContract {
            // ETH balances, as a ghost mapping every contract carries
            con.state.push(StateVar {
                name: "__balance".into(),
                ty: Ty::Map(Box::new(Ty::Addr), Box::new(Ty::Uint(256))),
                init: None,
                constant: false,
                immutable: false,
                ghost: true,
                ghost_decl: None,
                transient: false,
            });
            // which code each address holds: a value of this epoch, which every external call and
            // contract creation moves on (code changes only through outside code), so code read
            // twice between calls (in the body and in a spec) is the same code
            con.state.push(StateVar {
                name: "__code_epoch".into(),
                ty: Ty::Uint(256),
                init: None,
                constant: false,
                immutable: false,
                ghost: true,
                ghost_decl: None,
                transient: false,
            });
            if con.kind == ContractKind::Contract
                && !con.funcs.iter().any(|f| f.name == "constructor")
            {
                // deployment must establish the invariants even without an explicit constructor
                let (item, _) = items[name];
                let fake: &'a ItemFunction<'a> = arena.alloc(ItemFunction {
                    kind: ast::FunctionKind::Constructor,
                    header: ast::FunctionHeader { span: item.span, ..Default::default() },
                    body: None,
                    body_span: item.span,
                });
                con.funcs.push(Func {
                    name: "constructor".into(),
                    contract: name.clone(),
                    item: fake,
                    requires: vec![],
                    ensures: vec![],
                    succeeds: vec![],
                    params: vec![],
                    rets: vec![],
                    spec_body: None,
                    checks: vec![],
                    opaque: false,
                    trusted: false,
                    synthetic: true,
                    orphans: vec![],
                    ghost_fn: false,
                });
            }
        }
        // An implementation of an interface function inherits its exported contract: the
        // interface's ensures (under its requires, as old(..) ==> ..) are proved of the body, so
        // what callers through the interface trust is checked wherever the implementation is here.
        let mut inherit: Vec<(usize, Spec<'a>)> = vec![];
        for old in &con.overridden {
            if old.item.body.is_some() || old.ensures.is_empty() {
                continue;
            }
            if !items.get(&old.contract).is_some_and(|(_, c)| c.kind == ContractKind::Interface) {
                continue;
            }
            let Some(i) = con.funcs.iter().position(|g| {
                g.name == old.name
                    && g.params.iter().map(|p| &p.1).eq(old.params.iter().map(|p| &p.1))
            }) else {
                continue;
            };
            let imp = &con.funcs[i];
            if imp.item.body.is_none() {
                continue;
            }
            let pn = |f: &Func<'_>| f.params.iter().map(|p| p.0.clone()).collect::<Vec<_>>();
            let rn = |f: &Func<'_>| f.rets.iter().map(|p| p.0.clone()).collect::<Vec<_>>();
            let named_ret = old.rets.iter().any(|r| !r.0.starts_with("__ret"));
            if pn(imp) != pn(old) || (named_ret && rn(imp) != rn(old)) {
                b.errors.push((
                    old.ensures[0].span,
                    format!(
                        "{}.{} implements {}.{} with other parameter or return names: use the interface's names so its contract can be checked",
                        imp.contract, imp.name, old.contract, old.name
                    ),
                ));
                continue;
            }
            let pre = if old.requires.is_empty() {
                None
            } else {
                Some(
                    old.requires
                        .iter()
                        .map(|r| format!("({})", r.text))
                        .collect::<Vec<_>>()
                        .join(" && "),
                )
            };
            for e in &old.ensures {
                let text = match &pre {
                    Some(p) => format!("old({p}) ==> ({})", e.text),
                    None => e.text.clone(),
                };
                if let Some(sp) = b.parse_spec(&text, e.span) {
                    inherit.push((i, sp));
                }
            }
        }
        for (i, sp) in inherit {
            con.funcs[i].ensures.push(sp);
        }
        contracts.push(con);
    }
    let mut program = Program {
        contracts,
        quants: b.quants,
        stmt_specs: b.stmt_specs,
        block_tail: b.block_tail,
        enums: b.enums,
        enum_variants: b.enum_variants,
        structs,
        udvts,
        struct_invs,
        decl_stmts: b.decl_stmts.clone(),
        const_sizes: b.const_sizes.clone(),
        errors: b.errors,
        ghost_errors: vec![],
    };
    let (ghost_errors, lemmas) = crate::ghost::check(&program);
    for c in &mut program.contracts {
        for f in c.funcs.iter_mut().chain(&mut c.overridden) {
            f.ghost_fn = lemmas.contains(&(f.contract.clone(), f.name.clone(), f.params.len()));
        }
    }
    program.ghost_errors = ghost_errors;
    program
}

/// The value of a fixed array's size expression: an integer literal, or a constant naming one.
pub(crate) fn array_size(consts: &FxHashMap<String, u64>, e: &ast::Expr<'_>) -> Option<u64> {
    use solar_parse::ast::{ExprKind, LitKind};
    match &e.kind {
        ExprKind::Lit(l, None) => match &l.kind {
            LitKind::Number(n) => u64::try_from(*n).ok(),
            _ => None,
        },
        ExprKind::Ident(i) => consts.get(i.as_str()).copied(),
        _ => None,
    }
}

/// A spec text that also runs as Solidity (it becomes a require under --instrument).
pub(crate) fn executable(text: &str) -> bool {
    !(text.contains("old(")
        || text.contains("==>")
        || text.contains("<==>")
        || text.contains("forall")
        || text.contains("exists"))
}

/// Summaries solar-verify trusts for well-known library functions it cannot verify itself, applied
/// only when the function carries no specs of its own and its parameters are named as upstream.
/// OpenZeppelin `Math.mulDiv`: the exact quotient of the 512-bit product, rounded as asked; it
/// reverts when `denominator` is zero or the result does not fit in `uint256`.
fn builtin_summary(cname: &str, f: &ItemFunction<'_>) -> Option<&'static str> {
    if cname != "Math"
        || f.header.name.map(|n| n.as_str() != "mulDiv").unwrap_or(true)
        || f.header.returns().len() != 1
    {
        return None;
    }
    let names: Vec<String> = f
        .header
        .parameters
        .iter()
        .map(|p| p.name.map(|n| n.as_str().to_string()).unwrap_or_default())
        .collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    match names.as_slice() {
        ["x", "y", "denominator"] => Some("denominator > 0 && result == x * y / denominator"),
        ["x", "y", "denominator", "rounding"] => Some(
            "denominator > 0 && result == ((rounding == Rounding.Ceil || rounding == Rounding.Expand) ? (x * y + denominator - 1) / denominator : x * y / denominator)",
        ),
        _ => None,
    }
}
