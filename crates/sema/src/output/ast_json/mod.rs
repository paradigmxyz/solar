//! solc's compact AST JSON.
//!
//! The AST has the source structure the JSON follows but no IDs, types or resolutions, so
//! [`AstJson`] walks the AST and reads those from the HIR in lockstep: the `i`th parameter, field
//! or modifier of an AST node is the `i`th one of the HIR node, and the items of a source or
//! contract are the HIR items without the getters and Yul functions lowering adds. Every node is a
//! small view that writes its fields straight to the serializer; nothing is cloned or collected
//! into a JSON value tree.
//!
//! The HIR side of every pair is optional: when the compilation stops before lowering the ASTs
//! (`stopAfter: parsing`), only the parsed AST exists, and the writers leave out what needs sema,
//! like solc. Its types are empty, and it has no resolutions, scopes or attributes that type
//! checking sets. Everything else is written from the AST, with or without sema.
//!
//! With sema, IDs agree with every other ID the compiler reports, such as the keys of
//! `immutableReferences` and the `astId` of the storage layout, but not with solc's numbering:
//!
//! - items, including variables, enum values and struct fields, use [`Hir::global_item_id`];
//! - source units follow the items, and import directives follow the source units;
//! - every other node takes the next ID of the range that its source reserves, when its writer
//!   reaches it, so a node writes the fields that need its ID, such as the scope of its children,
//!   after allocating it. Without sema, that is every node.
//!
//! A node's writer states its fields in order inside [`Cx::write`], [`Cx::write_item`] or a similar
//! method: the method starts the node (`id`, `src`, `nodeType`), passes a [`Node`] to a closure and
//! ends it. [`Node::entry`] and [`Node::opt`] chain without `?`, since the node keeps the first
//! error.
//!
//! Statements and expressions are in [`expr`], and inline assembly in [`yul`].
//!
//! [`Hir::global_item_id`]: crate::hir::Hir::global_item_id

use crate::{
    ast_lowering::resolve::Declaration,
    hir::{self, ContractId, ItemId, Res, SourceId, VariableId},
    ty::Gcx,
    typeck::override_checker::is_fully_implemented,
};
use alloy_primitives::hex;
use rayon::prelude::*;
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use serde_json::value::RawValue;
use solar_ast::{self as ast, CommentKind, DocComments, token::TokenLitKind};
use solar_data_structures::index::IndexVec;
use solar_interface::{
    BytePos, Ident, Span, Symbol, config::CompilerStage, kw, source_map::SourceFile,
};
use solar_parse::Lexer;
use std::{borrow::Cow, cell::Cell, collections::BTreeMap, fmt, sync::OnceLock};

/// Destructures the HIR node that pairs with an AST node. Without sema, there is none: the result
/// is `None`, or a `None` for each element of the tuple that the arm builds from a node of the
/// expected kind.
macro_rules! paired {
    (@none $out:expr) => {
        None
    };
    ($hir:expr, $pat:pat => ($($out:expr),+ $(,)?)) => {
        match $hir {
            Some($pat) => ($(Some($out)),+),
            None => ($(paired!(@none $out)),+),
            Some(other) => unreachable!("lowering does not match the AST: {other:?}"),
        }
    };
    ($hir:expr, $pat:pat => $out:expr) => {
        match $hir {
            Some($pat) => Some($out),
            None => None,
            Some(other) => unreachable!("lowering does not match the AST: {other:?}"),
        }
    };
}

mod expr;
mod ty;
mod yul;

impl<'gcx> Gcx<'gcx> {
    /// Returns the AST JSON of the compilation.
    ///
    /// Matches the `ast` output of solc's Standard JSON and combined JSON. `pretty` says whether
    /// the output is indented, which takes more work to write than compact output.
    ///
    /// The compilation either passed analysis, or stopped before lowering the ASTs. Only the
    /// former gives the nodes their IDs, types and resolutions; the latter gives the parsed AST,
    /// like solc with `stopAfter: parsing`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/develop/using-the-compiler.html#output-description>
    pub fn ast_json(self, pretty: bool) -> AstJson<'gcx> {
        let sema = self.sess.opts.stop_after.is_none_or(|stage| stage >= CompilerStage::Analysis);
        let hir = &self.hir;
        // Items, source units and import directives have the IDs that the compiler reports
        // elsewhere, which only exist with sema.
        let mut next_id = if sema { hir.item_count() + hir.sources.len() } else { 0 };
        let first_imports = if sema {
            hir.sources()
                .map(|source| {
                    let first = next_id;
                    next_id += source.imports.len();
                    first
                })
                .collect()
        } else {
            IndexVec::new()
        };
        AstJson {
            gcx: self,
            sema,
            first_imports,
            first_node: next_id,
            pretty,
            rendered: self.sources.iter().map(|_| OnceLock::new()).collect(),
        }
    }
}

/// The AST JSON of the sources of a compilation. See [`Gcx::ast_json`].
///
/// The IDs of the nodes that are no items interleave: the `i`th such node of source `s` of `n`
/// has the ID `first + s + i * n`. They are unique, and do not depend on the order in which the
/// sources are written, or on the number of threads that write them.
pub struct AstJson<'gcx> {
    gcx: Gcx<'gcx>,
    /// Whether the compilation passed analysis, which gives the nodes IDs, types and resolutions.
    sema: bool,
    /// The ID of the first import directive of each source, with sema.
    first_imports: IndexVec<SourceId, usize>,
    /// The first ID for the nodes that are no items, source units or import directives.
    first_node: usize,
    /// Whether the output is indented. Compact output caches the written descriptions of types,
    /// and writes sources ahead in parallel.
    pretty: bool,
    /// The sources written ahead by [`AstJson::render`].
    rendered: IndexVec<SourceId, OnceLock<Box<RawValue>>>,
}

impl<'gcx> AstJson<'gcx> {
    /// Returns the AST JSON of the source, or `None` if it has no AST, or if its imports were not
    /// resolved (`-Zno-resolve-imports`), since an import directive needs the source it imports.
    pub fn source(&self, id: SourceId) -> Option<SourceAstJson<'_, 'gcx>> {
        let source = &self.gcx.sources[id];
        let ast = source.ast.as_ref()?;
        let imports = ast.items.iter().filter(|item| matches!(item.kind, ast::ItemKind::Import(_)));
        (imports.count() == source.imports.len()).then_some(SourceAstJson { json: self, id })
    }

    /// Writes the sources ahead, in parallel on the thread pool of the session, so that writing
    /// the output only copies them. Compact output only: indentation depends on where a source
    /// ends up in the output.
    pub fn render(&self, sources: &[SourceAstJson<'_, 'gcx>]) {
        if self.pretty {
            return;
        }
        sources.par_iter().for_each(|source| {
            if let Ok(raw) = serde_json::value::to_raw_value(source) {
                let _ = self.rendered[source.id].set(raw);
            }
        });
    }

    /// Returns the ID of a source unit, with sema.
    fn source_unit_id(&self, id: SourceId) -> usize {
        self.gcx.hir.source_unit_id(id)
    }
}

/// The AST JSON of a source: the `SourceUnit` node.
#[derive(Clone, Copy)]
pub struct SourceAstJson<'a, 'gcx> {
    json: &'a AstJson<'gcx>,
    id: SourceId,
}

impl fmt::Debug for SourceAstJson<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceAstJson").field("id", &self.id).finish_non_exhaustive()
    }
}

impl Serialize for SourceAstJson<'_, '_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self { json, id } = *self;
        if let Some(rendered) = json.rendered[id].get() {
            return rendered.serialize(s);
        }
        let gcx = json.gcx;
        let source = &gcx.sources[id];
        let file = &source.file;
        let ids = Ids { next: Cell::new(json.first_node + id.index()), stride: gcx.sources.len() };
        let types = (!json.pretty).then(Default::default);
        let unit_id = if json.sema { json.source_unit_id(id) } else { ids.fresh() };
        let cx = Cx {
            json,
            ids: &ids,
            types: types.as_ref(),
            gcx,
            source: id,
            start: file.start_pos,
            in_contract: false,
            in_event: false,
            in_mapping_key: false,
            in_local: false,
            block: unit_id,
            returns: None,
            parent: expr::Parent::default(),
            for_init: false,
        };
        let ast = source.ast.as_ref().unwrap();

        let mut m = Node::object(s)?;
        m.entry("id", &unit_id);
        let start = source_start(gcx, file);
        let (lo, hi) = (start.to_u32() - file.start_pos.to_u32(), file.source_len.to_u32());
        m.entry("src", &format_args!("{lo}:{}:{}", hi - lo, id.index()));
        m.entry("nodeType", "SourceUnit");
        m.entry("absolutePath", &file.name.display().to_string());
        if json.sema {
            m.entry("exportedSymbols", &cx.w(ExportedSymbols));
        }
        if let Some(license) = license(file, &ast.items.raw) {
            m.entry("license", &license);
        }
        let hir = json.sema.then(|| gcx.hir.source(id));
        m.entry(
            "nodes",
            &cx.w(Items {
                ast: &ast.items.raw,
                hir: hir.map(|source| source.items),
                usings: hir.map_or(&[], |source| source.usings),
                imports: &source.imports,
            }),
        );
        m.end()
    }
}

/// The IDs of a source for its nodes that are no items, source units or import directives.
struct Ids {
    next: Cell<usize>,
    /// The number of sources, which each take every `stride`th ID.
    stride: usize,
}

impl Ids {
    fn fresh(&self) -> usize {
        self.next.replace(self.next.get() + self.stride)
    }
}

/// Returns where the first token of a source starts.
fn source_start(gcx: Gcx<'_>, file: &SourceFile) -> BytePos {
    let mut tokens = Lexer::from_source_file(gcx.sess, file);
    loop {
        let Some(token) = tokens.next() else { break file.end_position() };
        if token.comment().is_none() {
            break token.span.lo();
        }
    }
}

/// Returns the SPDX license identifier of a source, if the text outside its top-level items holds
/// exactly one.
///
/// Reference: `Parser::findLicenseString` in solc's `Parser.cpp`.
fn license(file: &SourceFile, items: &[ast::Item<'_>]) -> Option<String> {
    const TAG: &str = "SPDX-License-Identifier:";
    let offset = |pos: BytePos| (pos - file.start_pos).to_usize();
    let mut gaps = Vec::with_capacity(items.len() + 1);
    let mut start = 0;
    for item in items {
        gaps.push(&file.src[start..offset(item.span.lo())]);
        start = offset(item.span.hi());
    }
    gaps.push(&file.src[start..]);
    // A license ends at the end of its line or comment, and is not empty.
    let mut licenses = gaps.into_iter().flat_map(|gap| gap.split(TAG).skip(1)).filter_map(|text| {
        let text = text.trim_start();
        let end = text.find(['\n', '\r']).into_iter().chain(text.find("*/")).min()?;
        Some(text[..end].trim()).filter(|license| !license.is_empty())
    });
    let license = licenses.next()?;
    licenses.next().is_none().then(|| license.to_string())
}

/// The writer of a source: the source being written and what its nodes need to refer to other
/// nodes.
#[derive(Clone, Copy)]
struct Cx<'a, 'gcx> {
    json: &'a AstJson<'gcx>,
    ids: &'a Ids,
    /// The written `typeDescriptions` that only depend on a type, which most of them do. Pretty
    /// output has none, since it indents inside them.
    types: Option<&'a ty::TypeCache<'gcx>>,
    gcx: Gcx<'gcx>,
    source: SourceId,
    /// The start of the source in the source map, which spans are relative to.
    start: BytePos,
    /// Whether the node is part of an event: solc marks every variable of an event `indexed` or
    /// not.
    in_event: bool,
    /// Whether the node is the key type of a mapping, which solc parses without the state
    /// mutability of `address`.
    in_mapping_key: bool,
    /// Whether the node is an item of a contract.
    in_contract: bool,
    /// Whether the node is the type of a local variable, whose array elements solc parses as
    /// expressions and so gives no state mutability to an `address`.
    in_local: bool,
    /// The ID of the node that declares the variables of the statements: the innermost block,
    /// `for` statement or `catch` clause.
    block: usize,
    /// The ID of the list of the return parameters of the function that holds the node.
    returns: Option<usize>,
    /// What the parent of an expression tells it.
    parent: expr::Parent<'gcx>,
    /// Whether the node is the initialization of a `for` statement.
    for_init: bool,
}

/// A node `T` that serializes itself in the context.
struct W<'a, 'gcx, T>(Cx<'a, 'gcx>, T);

/// A sequence that serializes the items of an iterator.
struct Seq<F>(F);

impl<F, I> Serialize for Seq<F>
where
    F: Fn() -> I,
    I: IntoIterator,
    I::Item: Serialize,
{
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq((self.0)())
    }
}

/// Pairs the nodes of an AST list with those of the HIR list, which is absent without sema.
fn zip_opt<'a, A, H>(
    ast: &'a [A],
    hir: Option<&'a [H]>,
) -> impl Iterator<Item = (&'a A, Option<&'a H>)> {
    ast.iter().enumerate().map(move |(i, ast)| (ast, hir.map(|hir| &hir[i])))
}

/// A `start:length:source` location.
struct Src {
    span: Span,
    start: BytePos,
    source: SourceId,
}

impl Serialize for Src {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.span.is_dummy() {
            return s.serialize_str("-1:-1:-1");
        }
        let lo = self.span.lo().to_u32() - self.start.to_u32();
        let len = self.span.hi().to_u32() - self.span.lo().to_u32();
        // Three numbers of up to ten digits, and two colons.
        let mut buf = [0u8; 32];
        let mut end = 0;
        for (i, number) in [lo, len, self.source.index() as u32].into_iter().enumerate() {
            if i > 0 {
                buf[end] = b':';
                end += 1;
            }
            end += write_u32(number, &mut buf[end..]);
        }
        s.serialize_str(std::str::from_utf8(&buf[..end]).expect("digits and colons are ASCII"))
    }
}

impl fmt::Display for Src {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.span.is_dummy() {
            return f.write_str("-1:-1:-1");
        }
        let lo = self.span.lo().to_u32() - self.start.to_u32();
        let len = self.span.hi().to_u32() - self.span.lo().to_u32();
        write!(f, "{lo}:{len}:{}", self.source.index())
    }
}

/// Writes the decimal digits of `n` at the start of `out`, and returns how many.
fn write_u32(mut n: u32, out: &mut [u8]) -> usize {
    let mut digits = [0u8; 10];
    let mut len = 0;
    loop {
        digits[len] = b'0' + (n % 10) as u8;
        len += 1;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    for (i, digit) in digits[..len].iter().rev().enumerate() {
        out[i] = *digit;
    }
    len
}

impl<'a, 'gcx> Cx<'a, 'gcx> {
    fn w<T>(self, node: T) -> W<'a, 'gcx, T> {
        W(self, node)
    }

    fn src(self, span: Span) -> Src {
        Src { span, start: self.start, source: self.source }
    }

    /// Starts a node with the next free ID.
    fn node<S: Serializer>(
        self,
        s: S,
        node_type: &str,
        span: Span,
    ) -> Result<(Node<S::SerializeMap>, usize), S::Error> {
        let id = self.ids.fresh();
        Ok((self.node_with_id(s, id, node_type, span)?, id))
    }

    /// Starts a node with the given ID.
    fn node_with_id<S: Serializer>(
        self,
        s: S,
        id: usize,
        node_type: &str,
        span: Span,
    ) -> Result<Node<S::SerializeMap>, S::Error> {
        let mut m = Node::object(s)?;
        m.entry("id", &id);
        m.entry("src", &self.src(span));
        m.entry("nodeType", node_type);
        Ok(m)
    }

    /// Writes a node with the next free ID: the fields that `fields` writes.
    fn write<S: Serializer>(
        self,
        s: S,
        node_type: &str,
        span: Span,
        fields: impl FnOnce(&mut Node<S::SerializeMap>),
    ) -> Result<S::Ok, S::Error> {
        let (mut m, _) = self.node(s, node_type, span)?;
        fields(&mut m);
        m.end()
    }

    /// Writes a node with the given ID: the fields that `fields` writes.
    fn write_with_id<S: Serializer>(
        self,
        s: S,
        id: usize,
        node_type: &str,
        span: Span,
        fields: impl FnOnce(&mut Node<S::SerializeMap>),
    ) -> Result<S::Ok, S::Error> {
        let mut m = self.node_with_id(s, id, node_type, span)?;
        fields(&mut m);
        m.end()
    }

    /// Writes the node of an item: the fields that `fields` writes.
    ///
    /// Without sema, an item has no ID of its own, but takes the next free one.
    fn write_item<S: Serializer>(
        self,
        s: S,
        id: Option<impl Into<ItemId>>,
        node_type: &str,
        span: Span,
        fields: impl FnOnce(&mut Node<S::SerializeMap>),
    ) -> Result<S::Ok, S::Error> {
        self.write_with_id(s, self.item_node(id), node_type, span, fields)
    }

    /// Returns the ID of the node of an item, which takes the next free ID without sema.
    fn item_node(self, id: Option<impl Into<ItemId>>) -> usize {
        id.map_or_else(|| self.ids.fresh(), |id| self.item_id(id))
    }

    /// Returns whether the compilation passed analysis, which gives the nodes IDs, types and
    /// resolutions.
    fn sema(self) -> bool {
        self.json.sema
    }

    /// Returns the source file.
    fn file(self) -> &'gcx SourceFile {
        &self.gcx.sources[self.source].file
    }

    /// Returns the span of the first token after `span`, which is empty if there is none.
    fn token_after(&self, span: Span) -> Span {
        let file = self.file();
        let rest = &file.src[(span.hi() - file.start_pos).to_usize()..];
        let mut tokens = Lexer::with_start_pos(self.gcx.sess, rest, span.hi());
        tokens
            .find(|token| !token.is_comment_or_doc())
            .map_or(span.shrink_to_hi(), |token| token.span)
    }

    /// Returns the ID of the import directive that declares a module alias.
    fn import_id(self, declaration: &Declaration) -> Option<usize> {
        let import = declaration.import?;
        Some(self.json.first_imports[import.source] + import.position as usize)
    }

    /// Returns the ID of the import directive that declares the module alias `alias` of `source`
    /// in the scope of `from`, which name resolution recorded in the declaration.
    fn module_alias_id(self, from: SourceId, alias: Ident, source: SourceId) -> Option<usize> {
        let declarations = self.gcx.symbol_resolver.source_scopes[from].resolve(alias)?;
        let declaration = declarations.iter().find(|decl| decl.res == Res::Namespace(source))?;
        self.import_id(declaration)
    }

    /// Returns the source text of a span.
    fn text(&self, span: Span) -> &str {
        let file = self.file();
        &file.src[(span.lo() - file.start_pos).to_usize()..(span.hi() - file.start_pos).to_usize()]
    }

    /// Returns the span without a trailing `;`, which solc leaves out of the location of a
    /// variable and of most statements.
    fn without_semicolon(self, span: Span) -> Span {
        match self.text(span).strip_suffix(';') {
            Some(text) => span.with_hi(span.lo() + text.trim_end().len() as u32),
            None => span,
        }
    }

    fn item_id(self, id: impl Into<ItemId>) -> usize {
        self.gcx.hir.global_item_id(id)
    }

    /// Returns the ID of the scope of an item: its contract or its source unit.
    fn scope(self, contract: Option<ContractId>) -> usize {
        contract.map_or_else(|| self.json.source_unit_id(self.source), |id| self.item_id(id))
    }
}

/// A JSON object under construction. The entries chain without `?`: the writer keeps the first
/// error and returns it from [`Node::end`].
struct Node<M: SerializeMap> {
    map: M,
    error: Option<M::Error>,
}

impl<M: SerializeMap> Node<M> {
    fn new(map: M) -> Self {
        Self { map, error: None }
    }

    /// Starts an object without a known length.
    fn object<S: Serializer<SerializeMap = M>>(s: S) -> Result<Self, S::Error> {
        s.serialize_map(None).map(Self::new)
    }

    fn entry(&mut self, key: &str, value: &(impl Serialize + ?Sized)) -> &mut Self {
        if self.error.is_none()
            && let Err(error) = self.map.serialize_entry(key, value)
        {
            self.error = Some(error);
        }
        self
    }

    /// Writes the `name` and `nameLocation` of a declaration.
    fn name(&mut self, cx: Cx<'_, '_>, name: impl Into<Option<Ident>>) -> &mut Self {
        let name = name.into();
        self.entry("name", &self::name(name))
            .entry("nameLocation", &cx.src(name.map_or(Span::DUMMY, |name| name.span)))
    }

    /// Writes the `documentation` of a node, if it has any.
    ///
    /// Like solc, this is the last group of doc comments, which a blank line or another comment
    /// ends.
    fn docs(&mut self, cx: Cx<'_, '_>, docs: &DocComments<'_>) -> &mut Self {
        match last_docs(cx, docs) {
            Some((span, text)) => self.entry("documentation", &cx.w(Doc { span, text })),
            None => self,
        }
    }

    /// Writes an entry that solc leaves out when it is null.
    fn opt(&mut self, key: &str, value: Option<impl Serialize>) -> &mut Self {
        match value {
            Some(value) => self.entry(key, &value),
            None => self,
        }
    }

    fn end(self) -> Result<M::Ok, M::Error> {
        match self.error {
            Some(error) => Err(error),
            None => self.map.end(),
        }
    }
}

/// A symbol.
struct Sym(Symbol);

impl Serialize for Sym {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0.as_str())
    }
}

/// Returns the name of a declaration, which is empty if it has none.
fn name(name: Option<Ident>) -> Sym {
    Sym(name.map_or(kw::Empty, |name| name.name))
}

/// Returns the span and the text of the last group of doc comments, unless the text is empty.
fn last_docs(cx: Cx<'_, '_>, docs: &DocComments<'_>) -> Option<(Span, String)> {
    let start = (1..docs.len())
        .rev()
        .find(|&i| {
            let (prev, doc) = (&docs[i - 1], &docs[i]);
            let gap = cx.text(prev.span.shrink_to_hi().with_hi(doc.span.lo()));
            !(prev.kind == CommentKind::Line
                && doc.kind == CommentKind::Line
                && gap.chars().all(char::is_whitespace)
                && gap.matches('\n').count() == 1)
        })
        .unwrap_or(0);
    let docs = &docs[start..];
    let text = doc_text(docs);
    if text.is_empty() {
        return None;
    }
    let span = Span::join_first_last(docs.iter().map(|doc| doc.span));
    Some((span, text))
}

/// Returns `true` for the whitespace that solc skips in doc comments, which excludes Unicode spaces
/// such as a non-breaking space.
fn is_ascii_space(c: char) -> bool {
    c.is_ascii_whitespace()
}

/// Returns the text of doc comments like solc: without the comment markers, and the whitespace and
/// stars that lead each line.
fn doc_text(docs: &[ast::DocComment<'_>]) -> String {
    let mut text = String::new();
    for (i, doc) in docs.iter().enumerate() {
        let symbol = doc.symbol.as_str();
        match doc.kind {
            CommentKind::Line => {
                if symbol.is_empty() {
                    continue;
                }
                if i > 0 {
                    text.push('\n');
                }
                text.push_str(if i == 0 {
                    symbol.trim_start_matches(is_ascii_space)
                } else {
                    symbol
                });
            }
            CommentKind::Block => {
                let lines = symbol.split('\n').collect::<Vec<_>>();
                for (i, line) in lines.iter().enumerate() {
                    let mut line = line.trim_start_matches(is_ascii_space).trim_end_matches('\r');
                    if i > 0 {
                        // A line that starts with `**` glues to the previous one, and the symbol of
                        // a comment that ends in `**/` ends in a line with a single `*`.
                        if line.starts_with("**") {
                            text.push_str(line);
                            continue;
                        }
                        if line == "*" && i == lines.len() - 1 {
                            text.push('*');
                            continue;
                        }
                        line = line.strip_prefix('*').unwrap_or(line);
                        if line.is_empty() {
                            continue;
                        }
                    }
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(line);
                }
            }
        }
    }
    text
}

/// The IDs of items, sorted.
fn sorted_ids(cx: Cx<'_, '_>, ids: impl IntoIterator<Item = impl Into<ItemId>>) -> Vec<usize> {
    let mut ids = ids.into_iter().map(|id| cx.item_id(id)).collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The `exportedSymbols` of a source: the names in its scope and their declarations.
struct ExportedSymbols;

impl Serialize for W<'_, '_, ExportedSymbols> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let cx = self.0;
        let gcx = cx.gcx;
        let mut symbols = gcx.symbol_resolver.source_scopes[cx.source].iter().collect::<Vec<_>>();
        symbols.sort_unstable_by(|(a, _), (b, _)| a.as_str().cmp(b.as_str()));
        let mut m = s.serialize_map(Some(symbols.len()))?;
        for (name, declarations) in symbols {
            let ids = declarations
                .iter()
                .filter_map(|decl| match decl.res {
                    hir::Res::Item(id) => Some(cx.item_id(id)),
                    hir::Res::Namespace(_) => cx.import_id(decl),
                    hir::Res::Builtin(_) | hir::Res::Err(_) => None,
                })
                .collect::<Vec<_>>();
            m.serialize_entry(&Sym(name), &ids)?;
        }
        m.end()
    }
}

/// The items of a source or a contract.
struct Items<'a, 'ast, 'gcx> {
    ast: &'a [ast::Item<'ast>],
    /// The HIR items, which also hold the getters and Yul functions lowering adds, with sema.
    hir: Option<&'a [ItemId]>,
    usings: &'a [hir::UsingDirective<'gcx>],
    /// The imports of the source.
    imports: &'a [(ast::ItemId, SourceId)],
}

impl<'gcx> Serialize for W<'_, 'gcx, Items<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, items) = self;
        let hir = &cx.gcx.hir;
        let mut hir_items = items.hir.map(|ids| {
            ids.iter().copied().filter(|&id| match id {
                ItemId::Function(id) => {
                    !hir.function(id).is_yul && hir.function(id).gettee.is_none()
                }
                _ => true,
            })
        });
        let mut usings = items.usings.iter();
        let mut imports = items.imports.iter().enumerate();
        macro_rules! next_hir {
            ($kind:ident) => {
                hir_items.as_mut().map(|items| match items.next() {
                    Some(ItemId::$kind(id)) => id,
                    id => unreachable!("lowering does not match the AST: {id:?}"),
                })
            };
        }

        let mut seq = s.serialize_seq(Some(items.ast.len()))?;
        for (index, item) in items.ast.iter().enumerate() {
            match &item.kind {
                ast::ItemKind::Pragma(pragma) => seq.serialize_element(&cx.w((item, pragma)))?,
                ast::ItemKind::Import(import) => {
                    let (position, &(ast_id, source)) = imports.next().unwrap();
                    debug_assert_eq!(ast_id.index(), index);
                    let id = match cx.sema() {
                        true => cx.json.first_imports[cx.source] + position,
                        false => cx.ids.fresh(),
                    };
                    seq.serialize_element(&cx.w(Import { item, import, id, source }))?;
                }
                ast::ItemKind::Using(using) => {
                    seq.serialize_element(&cx.w((item, using, usings.next())))?
                }
                ast::ItemKind::Contract(contract) => {
                    seq.serialize_element(&cx.w((item, contract, next_hir!(Contract))))?
                }
                ast::ItemKind::Function(function) => {
                    seq.serialize_element(&cx.w((item, function, next_hir!(Function))))?
                }
                ast::ItemKind::Variable(variable) => {
                    let id = next_hir!(Variable);
                    // File-level variables are no state variables, so solc leaves their
                    // documentation out.
                    let docs = match id {
                        Some(id) => hir.variable(id).is_state_variable().then_some(&item.docs),
                        None => None,
                    };
                    let scope = id.map_or(cx.block, |id| cx.scope(hir.variable(id).contract));
                    seq.serialize_element(&cx.w(Var {
                        ast: variable,
                        id,
                        node: None,
                        value: true,
                        docs,
                        scope,
                    }))?
                }
                ast::ItemKind::Struct(strukt) => {
                    seq.serialize_element(&cx.w((item, strukt, next_hir!(Struct))))?
                }
                ast::ItemKind::Enum(enumm) => {
                    seq.serialize_element(&cx.w((item, enumm, next_hir!(Enum))))?
                }
                ast::ItemKind::Udvt(udvt) => {
                    seq.serialize_element(&cx.w((item, udvt, next_hir!(Udvt))))?
                }
                ast::ItemKind::Error(error) => {
                    seq.serialize_element(&cx.w((item, error, next_hir!(Error))))?
                }
                ast::ItemKind::Event(event) => {
                    seq.serialize_element(&cx.w((item, event, next_hir!(Event))))?
                }
            }
        }
        seq.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::PragmaDirective<'_>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, _)) = self;
        cx.write(s, "PragmaDirective", item.span, |m| {
            // The tokens between `pragma` and `;`, which keep the structure of the pragma that the
            // AST does not.
            let text = cx.text(item.span);
            let tokens = Lexer::with_start_pos(cx.gcx.sess, text, item.span.lo())
                .filter(|token| !token.is_comment_or_doc())
                .collect::<Vec<_>>();
            let literals = Seq(|| {
                tokens[1..tokens.len() - 1].iter().map(|token| match token.lit() {
                    Some(lit) if lit.kind == TokenLitKind::Str => {
                        Cow::Owned(lit.symbol.as_str().to_string())
                    }
                    _ => Cow::Borrowed(cx.text(token.span)),
                })
            });
            m.entry("literals", &literals);
        })
    }
}

/// An import directive.
struct Import<'a, 'ast> {
    item: &'a ast::Item<'ast>,
    import: &'a ast::ImportDirective<'ast>,
    id: usize,
    /// The imported source.
    source: SourceId,
}

impl Serialize for W<'_, '_, Import<'_, '_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Import { item, import, id, source }) = self;
        let json = cx.json;
        cx.write_with_id(s, *id, "ImportDirective", item.span, |m| {
            m.entry("file", &Sym(import.path.value));
            if cx.sema() {
                m.entry("sourceUnit", &json.source_unit_id(*source));
                m.entry("scope", &json.source_unit_id(cx.source));
            }
            let file_name = &cx.gcx.sources[*source].file.name;
            m.entry("absolutePath", &format_args!("{}", file_name.display()));
            let alias = import.source_alias();
            m.entry("unitAlias", &name(alias));
            m.entry("nameLocation", &cx.src(alias.map_or(Span::DUMMY, |alias| alias.span)));
            let aliases = match &import.items {
                ast::ImportItems::Aliases(aliases) => &aliases[..],
                _ => &[],
            };
            let aliases = Seq(|| {
                aliases.iter().map(|&(foreign, local)| {
                    cx.w(SymbolAlias { foreign, local, source: *source, name: alias })
                })
            });
            m.entry("symbolAliases", &aliases);
        })
    }
}

/// A symbol of an import, `foreign as local`.
struct SymbolAlias {
    foreign: Ident,
    local: Option<Ident>,
    /// The imported source.
    source: SourceId,
    /// The alias of the whole import.
    name: Option<Ident>,
}

impl Serialize for W<'_, '_, SymbolAlias> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, SymbolAlias { foreign, local, source, name }) = self;
        let mut m = Node::object(s)?;
        m.entry("foreign", &cx.w((*foreign, *source)));
        if let Some(local) = local {
            m.entry("local", &Sym(local.name));
        }
        // solc reports the location of the import's own name, not of the symbol's.
        m.entry("nameLocation", &cx.src(name.map_or(Span::DUMMY, |name| name.span)));
        m.end()
    }
}

/// The `Identifier` node of an imported symbol.
impl Serialize for W<'_, '_, (Ident, SourceId)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (foreign, imported)) = self;
        cx.write(s, "Identifier", foreign.span, |m| {
            m.entry("name", foreign.as_str());
            m.entry("overloadedDeclarations", &[(); 0]);
            let declaration = cx.sema().then(|| {
                let declarations =
                    cx.gcx.symbol_resolver.source_scopes[*imported].resolve(*foreign);
                match declarations? {
                    [declaration] => match declaration.res {
                        hir::Res::Item(id) => Some(cx.item_id(id)),
                        hir::Res::Namespace(_) => cx.import_id(declaration),
                        _ => None,
                    },
                    _ => None,
                }
            });
            m.opt("referencedDeclaration", declaration.flatten());
            m.entry("typeDescriptions", &Empty);
        })
    }
}

/// An empty JSON object.
struct Empty;

impl Serialize for Empty {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_map(Some(0))?.end()
    }
}

impl<'gcx> Serialize
    for W<'_, 'gcx, (&ast::Item<'_>, &ast::UsingDirective<'_>, Option<&hir::UsingDirective<'gcx>>)>
{
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, using, hir)) = self;
        cx.write(s, "UsingForDirective", item.span, |m| {
            if let Some(ty) = &using.ty {
                m.entry("typeName", &cx.w((ty, hir.and_then(|hir| hir.ty.as_ref()))));
            }
            match &using.list {
                ast::UsingList::Single(path) => {
                    let library = hir.and_then(|hir| match hir.entries[0].kind {
                        hir::UsingEntryKind::Library(id) => Some(cx.item_id(id)),
                        _ => None,
                    });
                    m.entry("libraryName", &cx.w(IdentPath { path, declaration: library }));
                }
                ast::UsingList::Multiple(paths) => {
                    let list = Seq(|| {
                        zip_opt(paths, hir.map(|hir| hir.entries)).map(
                            |((path, operator), entry)| {
                                let function = entry.and_then(|entry| match entry.kind {
                                    hir::UsingEntryKind::Functions(&[id]) => Some(cx.item_id(id)),
                                    _ => None,
                                });
                                cx.w(UsingFunction { path, function, operator: *operator })
                            },
                        )
                    });
                    m.entry("functionList", &list);
                }
            }
            m.entry("global", &using.global);
        })
    }
}

/// An entry of the braced list of a `using` directive: a function, or an operator definition.
struct UsingFunction<'a, 'ast> {
    path: &'a ast::AstPath<'ast>,
    function: Option<usize>,
    operator: Option<ast::UserDefinableOperator>,
}

impl Serialize for W<'_, '_, UsingFunction<'_, '_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, UsingFunction { path, function, operator }) = self;
        let mut m = Node::object(s)?;
        let path = cx.w(IdentPath { path, declaration: *function });
        match operator {
            None => {
                m.entry("function", &path);
            }
            Some(operator) => {
                m.entry("definition", &path).entry("operator", operator.to_str());
            }
        }
        m.end()
    }
}

/// A qualified name and the declaration it refers to.
struct IdentPath<'a> {
    path: &'a ast::PathSlice,
    declaration: Option<usize>,
}

impl Serialize for W<'_, '_, IdentPath<'_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, IdentPath { path, declaration }) = self;
        cx.write(s, "IdentifierPath", path.span(), |m| {
            m.entry("name", &format_args!("{path}"));
            let locations = Seq(|| path.segments().iter().map(|segment| cx.src(segment.span)));
            m.entry("nameLocations", &locations);
            if let Some(declaration) = declaration {
                m.entry("referencedDeclaration", declaration);
            }
        })
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemContract<'_>, Option<ContractId>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, contract, id)) = self;
        let gcx = cx.gcx;
        let c = id.map(|id| gcx.hir.contract(id));
        cx.write_item(s, *id, "ContractDefinition", item.span, |m| {
            m.docs(*cx, &item.docs);
            m.name(*cx, contract.name);
            m.entry(
                "contractKind",
                match contract.kind {
                    ast::ContractKind::Interface => "interface",
                    ast::ContractKind::Library => "library",
                    ast::ContractKind::Contract | ast::ContractKind::AbstractContract => "contract",
                },
            );
            m.entry("abstract", &contract.kind.is_abstract_contract());
            let bases = Seq(|| {
                zip_opt(contract.bases, c.map(|c| c.bases_args))
                    .map(|(base, hir)| cx.w(Inheritance(base, hir)))
            });
            m.entry("baseContracts", &bases);
            if cx.sema() {
                m.entry("canonicalName", contract.name.as_str());
            }
            // Without sema, the lists are empty.
            let deps = id.map(|id| gcx.contract_bytecode_dependencies(id));
            m.entry(
                "contractDependencies",
                &sorted_ids(*cx, deps.iter().flat_map(|deps| deps.iter())),
            );
            let events = id.map(|id| gcx.interface_events(id));
            m.entry("usedEvents", &sorted_ids(*cx, events.iter().flat_map(|events| events.iter())));
            let errors = id.map(|id| gcx.interface_errors(id));
            m.entry("usedErrors", &sorted_ids(*cx, errors.iter().flat_map(|errors| errors.iter())));
            let items = Items {
                ast: contract.body,
                hir: c.map(|c| c.items),
                usings: c.map_or(&[], |c| c.usings),
                imports: &[],
            };
            m.entry("nodes", &Cx { in_contract: true, ..*cx }.w(items));
            if cx.sema() {
                m.entry("scope", &cx.scope(None));
            }
            if let Some(layout) = &contract.layout {
                let slot = c.and_then(|c| c.layout);
                m.entry("storageLayout", &cx.w(expr::StorageLayout(layout, slot)));
            }
            if let Some((id, c)) = id.zip(c) {
                m.entry("fullyImplemented", &is_fully_implemented(gcx, id));
                if !c.linearization_failed() {
                    let linearized = c.linearized_bases.iter().map(|&base| cx.item_id(base));
                    m.entry("linearizedBaseContracts", &linearized.collect::<Vec<_>>());
                }
                // Functions that function pointers reach are numbered from 1 in the order solc
                // creates their nodes: by source name, then by position.
                let mut targets =
                    gcx.contract_internal_dispatch_targets(id).iter().collect::<Vec<_>>();
                targets.sort_by_cached_key(|&function| {
                    let function = gcx.hir.function(function);
                    (
                        gcx.hir.source(function.source).file.name.display().to_string(),
                        function.span.lo(),
                    )
                });
                if !targets.is_empty() {
                    let ids = targets
                        .iter()
                        .zip(1..)
                        .map(|(&function, n)| (cx.item_id(function).to_string(), n));
                    m.entry("internalFunctionIDs", &ids.collect::<BTreeMap<_, usize>>());
                }
            }
        })
    }
}

/// A base contract in the inheritance list: an `InheritanceSpecifier` node.
struct Inheritance<'a, 'ast, 'gcx>(&'a ast::Modifier<'ast>, Option<&'a hir::Modifier<'gcx>>);

impl<'gcx> Serialize for W<'_, 'gcx, Inheritance<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Inheritance(base, hir)) = self;
        cx.write(s, "InheritanceSpecifier", base.span(), |m| {
            let declaration = hir.map(|hir| cx.item_id(hir.id));
            m.entry("baseName", &cx.w(IdentPath { path: &base.name, declaration }));
            if !base.arguments.is_dummy() {
                m.entry("arguments", &cx.w(expr::Args(&base.arguments, hir.map(|hir| &hir.args))));
            }
        })
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemFunction<'_>, Option<hir::FunctionId>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, function, id)) = self;
        let gcx = cx.gcx;
        let f = id.map(|id| gcx.hir.function(id));
        let header = &function.header;
        let item_id = cx.item_node(*id);
        let is_modifier = function.kind.is_modifier();
        let node_type = if is_modifier { "ModifierDefinition" } else { "FunctionDefinition" };
        let mut m = cx.node_with_id(s, item_id, node_type, item.span)?;
        m.docs(*cx, &item.docs);
        m.name(*cx, header.name);
        // The `return` statements of the body refer to the list of return parameters.
        let returns_id = (!is_modifier).then(|| cx.ids.fresh());
        if !is_modifier {
            let free = !cx.in_contract && function.kind.is_ordinary();
            m.entry("kind", if free { "freeFunction" } else { function.kind.to_str() });
            m.entry("stateMutability", header.state_mutability().to_str());
        }
        m.entry("virtual", &header.virtual_());
        if let Some(overrides) = &header.override_ {
            m.entry("overrides", &cx.w((overrides, f.map(|f| f.overrides))));
        }
        // A modifier without `()` has an empty list at the token after its name.
        let span = match header.name {
            Some(name) if header.parameters.span.is_dummy() => {
                cx.token_after(name.span).shrink_to_lo()
            }
            _ => header.parameters.span,
        };
        let parameters = Params {
            id: None,
            span,
            ast: header.parameters.vars,
            hir: f.map(|f| f.parameters),
            scope: item_id,
        };
        m.entry("parameters", &cx.w(parameters));
        if !is_modifier {
            // The empty list of a function without `returns` sits where its body starts.
            let span =
                header.returns.as_ref().map_or(function.body_span.shrink_to_lo(), |r| r.span);
            let returns = Params {
                id: returns_id,
                span,
                ast: header.returns(),
                hir: f.map(|f| f.returns),
                scope: item_id,
            };
            m.entry("returnParameters", &cx.w(returns));
            let modifiers = Seq(|| {
                zip_opt(header.modifiers, f.map(|f| f.modifiers))
                    .map(|(modifier, hir)| cx.w((modifier, hir)))
            });
            m.entry("modifiers", &modifiers);
            m.entry("implemented", &function.is_implemented());
            if let Some(f) = f {
                m.entry("scope", &cx.scope(f.contract));
            }
        }
        // A constructor of an abstract contract can only be called by derived contracts. Without
        // sema, it has no visibility.
        let visibility = match function.kind.is_constructor() {
            true => f
                .and_then(|f| f.contract)
                .map(|c| if gcx.hir.contract(c).is_abstract() { "internal" } else { "public" }),
            false => Some(function.visibility_or_default(!cx.in_contract).to_str()),
        };
        m.opt("visibility", visibility);
        if let Some(ast_body) = &function.body {
            let body = expr::Block {
                docs: None,
                span: ast_body.span,
                ast: ast_body,
                hir: f.and_then(|f| f.body.as_ref()),
                unchecked: false,
            };
            m.entry("body", &Cx { returns: returns_id, ..*cx }.w(body));
        }
        if let Some((id, f)) = id.zip(f) {
            if f.is_part_of_external_interface() {
                m.entry("functionSelector", &hex::encode(gcx.function_selector(id)));
            }
            let bases = gcx.base_override_items(id.into());
            if !bases.is_empty() {
                let key = if is_modifier { "baseModifiers" } else { "baseFunctions" };
                m.entry(key, &sorted_ids(*cx, bases.iter().copied()));
            }
        }
        m.end()
    }
}

impl<'gcx> Serialize for W<'_, 'gcx, (&ast::Modifier<'_>, Option<&hir::Modifier<'gcx>>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (modifier, hir)) = self;
        cx.write(s, "ModifierInvocation", modifier.span(), |m| {
            let declaration = hir.map(|hir| cx.item_id(hir.id));
            m.entry("modifierName", &cx.w(IdentPath { path: &modifier.name, declaration }));
            if !modifier.arguments.is_dummy() {
                m.entry(
                    "arguments",
                    &cx.w(expr::Args(&modifier.arguments, hir.map(|hir| &hir.args))),
                );
            }
            if let Some(hir) = hir {
                let kind = match hir.id {
                    ItemId::Contract(_) => "baseConstructorSpecifier",
                    _ => "modifierInvocation",
                };
                m.entry("kind", kind);
            }
        })
    }
}

impl Serialize for W<'_, '_, (&ast::Override<'_>, Option<&[ContractId]>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (overrides, ids)) = self;
        cx.write(s, "OverrideSpecifier", overrides.span, |m| {
            let paths = Seq(|| {
                zip_opt(overrides.paths, *ids).map(|(path, id)| {
                    cx.w(IdentPath { path, declaration: id.map(|&id| cx.item_id(id)) })
                })
            });
            m.entry("overrides", &paths);
        })
    }
}

/// A list of parameters: a `ParameterList` node.
struct Params<'a, 'ast> {
    /// The ID of the node, if it was allocated before.
    id: Option<usize>,
    span: Span,
    ast: &'a [ast::VariableDefinition<'ast>],
    hir: Option<&'a [VariableId]>,
    /// The ID of the node that declares the parameters, with sema.
    scope: usize,
}

impl Serialize for W<'_, '_, Params<'_, '_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Params { id, span, ast, hir, scope }) = self;
        let id = id.unwrap_or_else(|| cx.ids.fresh());
        cx.write_with_id(s, id, "ParameterList", *span, |m| {
            m.entry("parameters", &cx.w(Vars { ast, hir: *hir, scope: *scope }));
        })
    }
}

/// The variables of a parameter list, a struct or an event.
struct Vars<'a, 'ast> {
    ast: &'a [ast::VariableDefinition<'ast>],
    hir: Option<&'a [VariableId]>,
    scope: usize,
}

impl Serialize for W<'_, '_, Vars<'_, '_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Vars { ast, hir, scope }) = self;
        s.collect_seq(zip_opt(ast, *hir).map(|(ast, id)| {
            let id = id.copied();
            cx.w(Var { ast, id, node: None, value: false, docs: None, scope: *scope })
        }))
    }
}

/// A variable of any kind.
struct Var<'a, 'ast> {
    ast: &'a ast::VariableDefinition<'ast>,
    /// The variable, with sema.
    id: Option<VariableId>,
    /// The ID of the node, if it was allocated before.
    node: Option<usize>,
    /// Whether the variable has its initial value, which a local variable leaves to its
    /// declaration statement.
    value: bool,
    /// The documentation, for state variables.
    docs: Option<&'a DocComments<'ast>>,
    /// The ID of the node that declares the variable.
    scope: usize,
}

impl Serialize for W<'_, '_, Var<'_, '_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Var { ast, id, node, value, docs, scope }) = self;
        let gcx = cx.gcx;
        let var = id.map(|id| gcx.hir.variable(id));
        let span = cx.without_semicolon(ast.span);
        // The end of a function type is the end of the next token, which an unnamed variable
        // of the type shares.
        let span = match ast.ty.kind {
            ast::TypeKind::Function(_) if ast.name.is_none() => {
                span.with_hi(cx.token_after(ast.ty.span).hi())
            }
            _ => span,
        };
        // A local variable leaves its initial value to the declaration statement.
        let span = match (&ast.initializer, ast.name) {
            (Some(_), Some(name)) if !*value => span.with_hi(name.span.hi()),
            _ => span,
        };
        let node = node.unwrap_or_else(|| cx.item_node(*id));
        cx.write_with_id(s, node, "VariableDeclaration", span, |m| {
            if let Some(docs) = docs {
                m.docs(*cx, docs);
            }
            m.name(*cx, ast.name);
            m.entry("typeName", &cx.w((&ast.ty, var.map(|var| &var.ty))));
            m.entry("constant", &(ast.mutability == Some(ast::VarMut::Constant)));
            m.entry("mutability", ast.mutability.map_or("mutable", ast::VarMut::to_str));
            // Without sema, solc does not know a state variable by its scope either.
            m.entry("stateVariable", &var.is_some_and(|var| var.is_state_variable()));
            m.entry("storageLocation", ast.data_location.map_or("default", |loc| loc.to_str()));
            if let Some(overrides) = &ast.override_ {
                m.entry("overrides", &cx.w((overrides, var.map(|var| var.overrides))));
            }
            m.entry("visibility", ast.visibility.unwrap_or(ast::Visibility::Internal).to_str());
            if *value && let Some(initializer) = &ast.initializer {
                m.entry("value", &cx.expr(initializer, var.and_then(|var| var.initializer)));
            }
            m.opt("scope", id.map(|_| *scope));
            if let Some((id, var)) = id.zip(var) {
                // solc marks the reference types of every variable but a storage state variable
                // as pointers.
                let pointer = !var.is_state_variable() || var.mutability.is_some();
                let ty = gcx.type_of_item(id.into());
                m.entry("typeDescriptions", &ty::TypeDescriptions::new(*cx, ty, pointer));
                if let Some(getter) = var.getter {
                    m.entry("functionSelector", &hex::encode(gcx.function_selector(getter)));
                    let bases = gcx.base_override_items(id.into());
                    if !bases.is_empty() {
                        m.entry("baseFunctions", &sorted_ids(*cx, bases.iter().copied()));
                    }
                }
            } else {
                m.entry("typeDescriptions", &Empty);
            }
            if cx.in_event {
                m.entry("indexed", &ast.indexed);
            }
        })
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemStruct<'_>, Option<hir::StructId>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, strukt, id)) = self;
        let gcx = cx.gcx;
        let hir = id.map(|id| gcx.hir.strukt(id));
        let item_id = cx.item_node(*id);
        cx.write_with_id(s, item_id, "StructDefinition", item.span, |m| {
            m.docs(*cx, &item.docs);
            m.name(*cx, strukt.name);
            m.entry("visibility", "public");
            let members =
                Vars { ast: strukt.fields, hir: hir.map(|hir| hir.fields), scope: item_id };
            m.entry("members", &cx.w(members));
            if let Some((id, hir)) = id.zip(hir) {
                m.entry("scope", &cx.scope(hir.contract));
                m.entry("canonicalName", &format_args!("{}", gcx.item_canonical_name(id)));
            }
        })
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemEnum<'_>, Option<hir::EnumId>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, enumm, id)) = self;
        let gcx = cx.gcx;
        cx.write_item(s, *id, "EnumDefinition", item.span, |m| {
            m.docs(*cx, &item.docs);
            m.name(*cx, enumm.name);
            let variants = id.map(|id| gcx.hir.enumm(id).variants);
            m.entry("members", &cx.w(EnumValues { ast: enumm.variants, hir: variants }));
            if let Some(id) = id {
                m.entry("canonicalName", &format_args!("{}", gcx.item_canonical_name(*id)));
            }
        })
    }
}

/// The variants of an enum: `EnumValue` nodes.
struct EnumValues<'a> {
    ast: &'a [Ident],
    hir: Option<&'a [VariableId]>,
}

impl Serialize for W<'_, '_, EnumValues<'_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, EnumValues { ast, hir }) = self;
        let mut seq = s.serialize_seq(Some(ast.len()))?;
        for (&variant, id) in zip_opt(ast, *hir) {
            seq.serialize_element(&cx.w((id.copied(), variant)))?;
        }
        seq.end()
    }
}

impl Serialize for W<'_, '_, (Option<VariableId>, Ident)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (id, name)) = self;
        cx.write_item(s, *id, "EnumValue", name.span, |m| {
            m.name(*cx, *name);
        })
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemUdvt<'_>, Option<hir::UdvtId>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, udvt, id)) = self;
        let gcx = cx.gcx;
        cx.write_item(s, *id, "UserDefinedValueTypeDefinition", item.span, |m| {
            m.name(*cx, udvt.name);
            let hir = id.map(|id| gcx.hir.udvt(id));
            m.entry("underlyingType", &cx.w((&udvt.ty, hir.map(|hir| &hir.ty))));
            if let Some(id) = id {
                m.entry("canonicalName", &format_args!("{}", gcx.item_canonical_name(*id)));
            }
        })
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemEvent<'_>, Option<hir::EventId>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, event, id)) = self;
        let gcx = cx.gcx;
        let hir = id.map(|id| gcx.hir.event(id));
        let item_id = cx.item_node(*id);
        cx.write_with_id(s, item_id, "EventDefinition", item.span, |m| {
            m.docs(*cx, &item.docs);
            m.name(*cx, event.name);
            let parameters = Params {
                id: None,
                span: event.parameters.span,
                ast: event.parameters.vars,
                hir: hir.map(|hir| hir.parameters),
                scope: item_id,
            };
            m.entry("parameters", &Cx { in_event: true, ..*cx }.w(parameters));
            m.entry("anonymous", &event.anonymous);
            if let Some(id) = id {
                m.entry("eventSelector", &hex::encode(gcx.event_selector(*id)));
            }
        })
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemError<'_>, Option<hir::ErrorId>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, error, id)) = self;
        let gcx = cx.gcx;
        let hir = id.map(|id| gcx.hir.error(id));
        let item_id = cx.item_node(*id);
        cx.write_with_id(s, item_id, "ErrorDefinition", item.span, |m| {
            m.docs(*cx, &item.docs);
            m.name(*cx, error.name);
            let parameters = Params {
                id: None,
                span: error.parameters.span,
                ast: error.parameters.vars,
                hir: hir.map(|hir| hir.parameters),
                scope: item_id,
            };
            m.entry("parameters", &cx.w(parameters));
            if let Some(id) = id {
                m.entry("errorSelector", &hex::encode(gcx.function_selector(*id)));
            }
        })
    }
}

/// The documentation of a node: a `StructuredDocumentation` node.
struct Doc {
    span: Span,
    text: String,
}

impl Serialize for W<'_, '_, Doc> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Doc { span, text }) = self;
        // solc includes the line break that ends a line comment, unless an indentation follows.
        let mut span = *span;
        let end = cx.file().end_position();
        let rest = cx.text(span.shrink_to_hi().with_hi(end));
        if cx.text(span).starts_with("//") {
            let line_break =
                if rest.starts_with("\r\n") { 2 } else { rest.starts_with(['\r', '\n']) as usize };
            if !rest[line_break..].starts_with([' ', '\t']) {
                span = span.with_hi(span.hi() + line_break as u32);
            }
        }
        cx.write(s, "StructuredDocumentation", span, |m| {
            m.entry("text", text);
        })
    }
}
