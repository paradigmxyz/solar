//! Loading a file to verify together with the files it imports.
//!
//! The verifier resolves names globally, as if the files were flattened into one. [`Loader::load`]
//! parses a file and, recursively, every file it imports, resolving import paths with the
//! compiler's file resolver (remappings, base path and include paths, as `solar` and solc take
//! them), and returns the parsed files with every file after the files it imports. Two readings
//! cannot be flattened, so they are errors that stop the file from being verified: an import
//! that namespaces what it imports (`* as`, `import "a.sol" as A`), and a name that two files
//! define. Two exceptions: structs that contracts or interfaces declare, which `prog::build`
//! keys by their qualified names, and an interface that several files declare alike (see
//! [`same_interface`]), which the program reads as one. Like solc, an interface of one name
//! that two files declare is still an error where one file's scope sees both.

use solar_parse::{
    Parser,
    ast::{self, ImportItems, ItemKind, SourceUnit},
    interface::{
        Session, Span,
        config::ImportRemapping,
        data_structures::map::{FxHashMap, FxHashSet},
        source_map::{FileName, FileResolver, SourceFile},
    },
    unescape::{self, StrKind},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// Where imports are looked up, as `solar` and solc take it on the command line.
#[derive(Clone, Debug, Default)]
pub(crate) struct ImportOpts {
    /// `prefix=path` and `context:prefix=path` remappings, as written.
    pub remappings: Vec<String>,
    pub base_path: Option<PathBuf>,
    pub include_paths: Vec<PathBuf>,
}

impl ImportOpts {
    /// Takes the option at `arg`, reading its value from `rest`; false when `arg` is not one.
    pub(crate) fn parse_arg(&mut self, arg: &str, rest: &mut impl Iterator<Item = String>) -> bool {
        match arg {
            "--base-path" => self.base_path = rest.next().map(PathBuf::from),
            "-I" | "--include-path" => self.include_paths.extend(rest.next().map(PathBuf::from)),
            _ if !arg.starts_with('-') && arg.contains('=') => self.remappings.push(arg.into()),
            _ => return false,
        }
        true
    }

    /// The options as arguments, for a child process.
    pub(crate) fn args(&self) -> Vec<String> {
        let mut args = self.remappings.clone();
        if let Some(base) = &self.base_path {
            args.extend(["--base-path".into(), base.display().to_string()]);
        }
        for path in &self.include_paths {
            args.extend(["--include-path".into(), path.display().to_string()]);
        }
        args
    }

    /// The remappings, or the first one that does not parse.
    pub(crate) fn parsed_remappings(&self) -> Result<Vec<ImportRemapping>, String> {
        self.remappings
            .iter()
            .map(|remapping| {
                remapping
                    .parse()
                    .map_err(|error| format!("invalid remapping `{remapping}`: {error}"))
            })
            .collect()
    }
}

/// A file to verify, parsed with the files it imports.
pub(crate) struct Loaded<'a> {
    /// The file to verify.
    pub file: Arc<SourceFile>,
    /// The parsed files, each after the files it imports, so the file to verify is last.
    pub units: Vec<&'a SourceUnit<'a>>,
}

/// Parses files and the files they import.
pub(crate) struct Loader<'s, 'a> {
    sess: &'s Session,
    arena: &'a ast::Arena,
    resolver: FileResolver<'s>,
}

impl<'s, 'a> Loader<'s, 'a> {
    pub(crate) fn new(
        sess: &'s Session,
        arena: &'a ast::Arena,
        opts: &ImportOpts,
    ) -> Result<Self, String> {
        let mut resolver = FileResolver::new(sess.source_map());
        resolver.add_import_remappings(opts.parsed_remappings()?);
        if let Some(base) = &opts.base_path {
            resolver.set_base_path(base);
        }
        resolver.add_include_paths(opts.include_paths.iter().cloned());
        Ok(Self { sess, arena, resolver })
    }

    /// `file` and everything it imports. With `mutant`, `file` holds what `mutant` holds instead
    /// of what is on disk, and imports resolve from where `file` is: this is how mutation testing
    /// verifies a mutant. Diagnostics are emitted; `Err` means they were.
    pub(crate) fn load(&self, file: &Path, mutant: Option<&Path>) -> Result<Loaded<'a>, ()> {
        let source_map = self.sess.source_map();
        let root = match mutant {
            Some(mutant) => source_map
                .file_loader()
                .load_file(mutant)
                .and_then(|text| source_map.new_source_file(FileName::Real(file.into()), text))
                .map_err(|error| error.to_string()),
            None => self.resolver.resolve_file(file, None).map_err(|error| error.to_string()),
        };
        let root = root.map_err(|error| {
            self.sess.dcx.err(format!("cannot read `{}`: {error}", file.display())).emit();
        })?;
        let mut load = Load {
            loader: self,
            seen: FxHashSet::default(),
            units: vec![],
            imports: vec![],
            ok: true,
        };
        load.visit(root.clone());
        let Load { units, imports, ok, .. } = load;
        let index: FxHashMap<Arc<SourceFile>, usize> =
            units.iter().enumerate().map(|(i, (file, _))| (file.clone(), i)).collect();
        let imports: Vec<_> = imports
            .into_iter()
            .filter_map(|(from, import, to)| Some((*index.get(&from)?, import, *index.get(&to)?)))
            .collect();
        let (files, units): (Vec<_>, Vec<_>) = units.into_iter().unzip();
        let unique = self.check_unique_names(&files, &units, &imports);
        if !ok {
            return Err(());
        }
        unique?;
        Ok(Loaded { file: root, units })
    }

    /// Reports names that two files define: resolving names globally would pick one of them.
    /// `imports` holds each file's imports, with the files they import, by index in `units`.
    fn check_unique_names(
        &self,
        files: &[Arc<SourceFile>],
        units: &[&'a SourceUnit<'a>],
        imports: &[(usize, &'a ast::ImportDirective<'a>, usize)],
    ) -> Result<(), ()> {
        let mut defined: FxHashMap<String, (usize, Span, Option<String>, &'a ast::Item<'a>)> =
            FxHashMap::default();
        // Interfaces two files declare alike, with their later declarations.
        let mut shared: FxHashMap<String, Vec<Span>> = FxHashMap::default();
        let mut ok = true;
        for (index, unit) in units.iter().enumerate() {
            // File-level names, and the enums and user-defined value types contracts declare,
            // which are looked up by bare name too. A contract's structs are keyed by their
            // qualified names (`prog::build`).
            let nested = unit.items.iter().flat_map(|item| match &item.kind {
                ItemKind::Contract(contract) => &contract.body[..],
                _ => &[],
            });
            let types =
                nested.filter(|item| matches!(item.kind, ItemKind::Enum(_) | ItemKind::Udvt(_)));
            for item in unit.items.iter().chain(types) {
                let Some(name) = item.name() else { continue };
                let defines = matches!(
                    item.kind,
                    ItemKind::Contract(_)
                        | ItemKind::Function(_)
                        | ItemKind::Variable(_)
                        | ItemKind::Struct(_)
                        | ItemKind::Enum(_)
                        | ItemKind::Udvt(_)
                        | ItemKind::Error(_)
                        | ItemKind::Event(_)
                );
                if !defines {
                    continue;
                }
                let constant = self.constant_text(item);
                match defined.entry(name.as_str().to_string()) {
                    // The same constant, of the same type and value, in two files (Seaport's
                    // constant files repeat words like `OneWord = 0x20`): either one reads alike.
                    std::collections::hash_map::Entry::Occupied(first)
                        if constant.is_some() && first.get().2 == constant => {}
                    std::collections::hash_map::Entry::Occupied(first)
                        if first.get().0 != index
                            && same_interface(self.sess, first.get().3, item) =>
                    {
                        shared.entry(name.as_str().to_string()).or_default().push(name.span);
                    }
                    std::collections::hash_map::Entry::Occupied(first)
                        if first.get().0 != index =>
                    {
                        self.sess
                            .dcx
                            .err(format!("`{name}` is defined in more than one file"))
                            .span(name.span)
                            .span_note(first.get().1, "also defined here")
                            .note("solar-verify resolves names as if the files were flattened")
                            .emit();
                        ok = false;
                    }
                    std::collections::hash_map::Entry::Occupied(_) => {}
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert((index, name.span, constant, item));
                    }
                }
            }
        }
        // An interface declared in two files is one only where no file sees both: solc rejects
        // a scope that sees two declarations of a name (`Identifier already declared`).
        if !shared.is_empty() {
            let scopes = scopes(units, imports);
            for (name, later) in &shared {
                let mut decls = vec![defined[name].1];
                decls.extend(later.iter().copied());
                for (file, scope) in files.iter().zip(&scopes) {
                    let Some(seen) = scope.get(name) else { continue };
                    let both: Vec<Span> =
                        decls.iter().copied().filter(|sp| seen.contains(sp)).collect();
                    let [also, at, ..] = both[..] else { continue };
                    self.sess
                        .dcx
                        .err(format!(
                            "`{name}` is declared twice where `{}` sees it",
                            file.name.display()
                        ))
                        .span(at)
                        .span_note(also, "also declared here")
                        .note("solc rejects a file that imports two declarations of one name")
                        .emit();
                    ok = false;
                }
            }
        }
        if ok { Ok(()) } else { Err(()) }
    }

    /// A file-level constant's type and initializer, as written without whitespace: two
    /// constants with the same text, whose other names resolve uniquely, have the same value.
    fn constant_text(&self, item: &ast::Item<'_>) -> Option<String> {
        let ItemKind::Variable(v) = &item.kind else { return None };
        if v.mutability != Some(ast::VarMut::Constant) {
            return None;
        }
        let map = self.sess.source_map();
        let ty = map.span_to_snippet(v.ty.span).ok()?;
        let init = map.span_to_snippet(v.initializer.as_deref()?.span).ok()?;
        Some(format!("{ty}={init}").chars().filter(|c| !c.is_whitespace()).collect())
    }
}

/// One traversal of a file's imports.
struct Load<'l, 's, 'a> {
    loader: &'l Loader<'s, 'a>,
    seen: FxHashSet<Arc<SourceFile>>,
    /// The parsed files, each after the files it imports.
    units: Vec<(Arc<SourceFile>, &'a SourceUnit<'a>)>,
    /// Each import: the file it is in, the directive, and the file it imports.
    imports: Vec<(Arc<SourceFile>, &'a ast::ImportDirective<'a>, Arc<SourceFile>)>,
    ok: bool,
}

impl<'s, 'a> Load<'_, 's, 'a> {
    fn visit(&mut self, file: Arc<SourceFile>) {
        if !self.seen.insert(file.clone()) {
            return;
        }
        let sess = self.loader.sess;
        let mut parser = Parser::from_source_file(sess, self.loader.arena, &file);
        let unit = match parser.parse_file() {
            Ok(unit) => &*self.loader.arena.alloc(unit),
            Err(error) => {
                error.emit();
                self.ok = false;
                return;
            }
        };
        let parent = match &file.name {
            FileName::Real(path) => path.as_path(),
            _ => Path::new(""),
        };
        for item in unit.items.iter() {
            let ItemKind::Import(import) = &item.kind else { continue };
            if !self.check_flattenable(import, item.span) {
                continue;
            }
            if let Some(imported) = self.resolve(import, parent) {
                self.imports.push((file.clone(), import, imported.clone()));
                self.visit(imported);
            }
        }
        self.units.push((file, unit));
    }

    /// Whether the import brings names in as they are, or renames single symbols (`{A as B}`,
    /// which `prog::build` resolves); namespaces are reported.
    fn check_flattenable(&mut self, import: &ast::ImportDirective<'_>, span: Span) -> bool {
        let namespaced = match &import.items {
            ImportItems::Plain(alias) => alias.is_some(),
            ImportItems::Aliases(_) => false,
            ImportItems::Glob(_) => true,
        };
        if namespaced {
            self.loader
                .sess
                .dcx
                .err("imports that namespace symbols are not supported")
                .span(span)
                .help("import the names: `import {A} from \"a.sol\";` or `import {A as B} from \"a.sol\";`")
                .emit();
            self.ok = false;
        }
        !namespaced
    }

    fn resolve(
        &mut self,
        import: &ast::ImportDirective<'_>,
        parent: &Path,
    ) -> Option<Arc<SourceFile>> {
        let sess = self.loader.sess;
        let span = import.path.span;
        let (bytes, any_error) =
            unescape::parse_string_literal(import.path.value.as_str(), StrKind::Str, span, sess);
        let path = (!any_error).then(|| std::str::from_utf8(&bytes).ok()).flatten();
        let Some(path) = path else {
            sess.dcx.err("import path is not a valid UTF-8 string").span(span).emit();
            self.ok = false;
            return None;
        };
        match self.loader.resolver.resolve_file(Path::new(path), Some(parent)) {
            Ok(file) => Some(file),
            Err(error) => {
                sess.dcx
                    .err(error.to_string())
                    .span(span)
                    .help("pass remappings as `prefix=path`, or `--base-path` and `--include-path`")
                    .emit();
                self.ok = false;
                None
            }
        }
    }
}

/// What a member of an interface (a function, event or error) declares to the code that uses
/// it: its kind, name, parameter and return types with their data locations, visibility and
/// state mutability, without parameter names or comments. None for any other member.
pub(crate) fn iface_member_shape(sess: &Session, item: &ast::Item<'_>) -> Option<String> {
    let map = sess.source_map();
    let var = |v: &ast::VariableDefinition<'_>| {
        let ty: String =
            map.span_to_snippet(v.ty.span).unwrap_or_default().split_whitespace().collect();
        format!("{ty} {:?} {}", v.data_location, v.indexed)
    };
    let list =
        |vs: &[ast::VariableDefinition<'_>]| vs.iter().map(var).collect::<Vec<_>>().join(",");
    match &item.kind {
        ItemKind::Function(f) => Some(format!(
            "{:?} {}({}) {:?} {:?} returns ({})",
            f.kind,
            f.header.name.map(|n| n.as_str().to_string()).unwrap_or_default(),
            list(&f.header.parameters),
            f.header.visibility(),
            f.header.state_mutability(),
            list(f.header.returns()),
        )),
        ItemKind::Event(e) => {
            Some(format!("event {}({}) {}", e.name.as_str(), list(&e.parameters), e.anonymous))
        }
        ItemKind::Error(e) => Some(format!("error {}({})", e.name.as_str(), list(&e.parameters))),
        _ => None,
    }
}

/// Whether `a` and `b` are interfaces of one name that read alike to the code that uses either:
/// neither has specs (`@custom:` tags) or members other than functions, events and errors, they
/// have the same bases, and every member name both declare has the same declarations in each
/// (`iface_member_shape`). Code compiled against either reaches only members it declares, so
/// the program can stand for both with the members of both: a member only one declares is never
/// reached through the other.
pub(crate) fn same_interface(sess: &Session, a: &ast::Item<'_>, b: &ast::Item<'_>) -> bool {
    let (ItemKind::Contract(ca), ItemKind::Contract(cb)) = (&a.kind, &b.kind) else {
        return false;
    };
    if !ca.kind.is_interface() || !cb.kind.is_interface() || ca.name.name != cb.name.name {
        return false;
    }
    let specs =
        |docs: &ast::DocComments<'_>| docs.iter().any(|d| d.symbol.as_str().contains("@custom:"));
    let bases = |c: &ast::ItemContract<'_>| -> Vec<String> {
        c.bases
            .iter()
            .map(|b| b.name.segments().iter().map(|s| s.as_str()).collect::<Vec<_>>().join("."))
            .collect()
    };
    // Each member name with its declarations; None when a member cannot be shared.
    let members = |it: &ast::Item<'_>, c: &ast::ItemContract<'_>| {
        if specs(&it.docs) {
            return None;
        }
        let mut out: FxHashMap<String, Vec<String>> = FxHashMap::default();
        for sub in c.body.iter() {
            let shape = iface_member_shape(sess, sub).filter(|_| !specs(&sub.docs))?;
            let name = sub.name().map(|n| n.as_str().to_string()).unwrap_or_default();
            out.entry(name).or_default().push(shape);
        }
        for shapes in out.values_mut() {
            shapes.sort();
        }
        Some(out)
    };
    let (Some(ma), Some(mb)) = (members(a, ca), members(b, cb)) else { return false };
    bases(ca) == bases(cb) && ma.iter().all(|(n, s)| mb.get(n).is_none_or(|t| s == t))
}

/// The names each file's scope sees, with the declarations (by name span) each stands for: its
/// own file-level declarations and what it imports, which for `import "a.sol"` is all that
/// `a.sol` sees.
fn scopes(
    units: &[&SourceUnit<'_>],
    imports: &[(usize, &ast::ImportDirective<'_>, usize)],
) -> Vec<FxHashMap<String, FxHashSet<Span>>> {
    let mut scopes: Vec<FxHashMap<String, FxHashSet<Span>>> = units
        .iter()
        .map(|u| {
            let mut own: FxHashMap<String, FxHashSet<Span>> = FxHashMap::default();
            for it in u.items.iter() {
                if let Some(n) = it.name() {
                    own.entry(n.as_str().to_string()).or_default().insert(n.span);
                }
            }
            own
        })
        .collect();
    // Imports may be circular: grow the scopes until none changes.
    loop {
        let mut grew = false;
        for &(from, import, to) in imports {
            let brought: Vec<(String, FxHashSet<Span>)> = match &import.items {
                ImportItems::Plain(None) => {
                    scopes[to].iter().map(|(n, s)| (n.clone(), s.clone())).collect()
                }
                ImportItems::Aliases(list) => list
                    .iter()
                    .filter_map(|(orig, alias)| {
                        let seen = scopes[to].get(orig.as_str())?;
                        Some((alias.unwrap_or(*orig).as_str().to_string(), seen.clone()))
                    })
                    .collect(),
                _ => vec![],
            };
            for (n, spans) in brought {
                let set = scopes[from].entry(n).or_default();
                let before = set.len();
                set.extend(spans);
                grew |= set.len() > before;
            }
        }
        if !grew {
            return scopes;
        }
    }
}
