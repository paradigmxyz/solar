//! solc's compact AST JSON.
//!
//! The AST has the source structure the JSON follows but no IDs, types or resolutions, so
//! [`AstJson`] walks the AST and reads those from the HIR in lockstep: the `i`th parameter, field
//! or modifier of an AST node is the `i`th one of the HIR node, and the items of a source or
//! contract are the HIR items without the getters and Yul functions lowering adds. Every node is a
//! small view that writes its fields straight to the serializer; nothing is cloned or collected
//! into a JSON value tree.
//!
//! IDs agree with every other ID the compiler reports, such as the keys of `immutableReferences`
//! and the `astId` of the storage layout, but not with solc's numbering:
//!
//! - items, including variables, enum values and struct fields, use [`Hir::global_item_id`];
//! - source units follow the items, and import directives follow the source units;
//! - every other node takes the next ID when its writer reaches it, so a node writes the fields
//!   that need its ID, such as the scope of its children, after allocating it.
//!
//! Statements and expressions are not supported yet: the bodies of functions and modifiers, the
//! arguments of modifier invocations and base constructor calls, array lengths, the initial values
//! of variables and the storage layout specifier are left out.
//!
//! [`Hir::global_item_id`]: crate::hir::Hir::global_item_id

use crate::{
    hir::{self, ContractId, ItemId, SourceId, VariableId},
    ty::Gcx,
    typeck::override_checker::is_fully_implemented,
};
use alloy_primitives::hex;
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use solar_ast::{self as ast, CommentKind, DocComments, token::TokenLitKind};
use solar_data_structures::index::IndexVec;
use solar_interface::{BytePos, Ident, Span, Symbol, kw, source_map::SourceFile};
use solar_parse::Lexer;
use std::{borrow::Cow, cell::Cell, fmt};

mod ty;

impl<'gcx> Gcx<'gcx> {
    /// Returns the AST JSON of the compilation, which must have passed analysis.
    ///
    /// Matches the `ast` output of solc's Standard JSON and combined JSON.
    ///
    /// Reference: <https://docs.soliditylang.org/en/develop/using-the-compiler.html#output-description>
    pub fn ast_json(self) -> AstJson<'gcx> {
        let hir = &self.hir;
        let mut next_import = hir.item_count() + hir.sources.len();
        let first_imports = hir
            .sources()
            .map(|source| {
                let first = next_import;
                next_import += source.imports.len();
                first
            })
            .collect();
        AstJson { gcx: self, next_id: Cell::new(next_import), first_imports }
    }
}

/// The AST JSON of the sources of a compilation. See [`Gcx::ast_json`].
///
/// Write each source once: the sources share the counter of the IDs that do not belong to items.
pub struct AstJson<'gcx> {
    gcx: Gcx<'gcx>,
    /// The ID of the next node that is not an item, a source unit or an import directive.
    next_id: Cell<usize>,
    /// The ID of the first import directive of each source.
    first_imports: IndexVec<SourceId, usize>,
}

impl<'gcx> AstJson<'gcx> {
    /// Returns the AST JSON of the source, or `None` if it has no AST.
    pub fn source(&self, id: SourceId) -> Option<SourceAstJson<'_, 'gcx>> {
        self.gcx.sources[id].ast.is_some().then_some(SourceAstJson { json: self, id })
    }

    fn fresh_id(&self) -> usize {
        self.next_id.replace(self.next_id.get() + 1)
    }

    fn source_unit_id(&self, id: SourceId) -> usize {
        self.gcx.hir.item_count() + id.index()
    }
}

/// The AST JSON of a source: the `SourceUnit` node.
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
        let gcx = json.gcx;
        let file = &gcx.hir.source(id).file;
        let cx = Cx {
            json,
            gcx,
            source: id,
            start: file.start_pos,
            in_event: false,
            in_mapping_key: false,
        };
        let ast = gcx.sources[id].ast.as_ref().unwrap();

        let mut m = s.serialize_map(None)?;
        m.serialize_entry("id", &json.source_unit_id(id))?;
        let (license, start) = license_and_start(gcx, file);
        let (lo, hi) = (start.to_u32() - file.start_pos.to_u32(), file.source_len.to_u32());
        m.serialize_entry("src", &format_args!("{lo}:{}:{}", hi - lo, id.index()))?;
        m.serialize_entry("nodeType", "SourceUnit")?;
        m.serialize_entry("absolutePath", &file.name.display().to_string())?;
        m.serialize_entry("exportedSymbols", &cx.w(ExportedSymbols))?;
        if let Some(license) = license {
            m.serialize_entry("license", &license)?;
        }
        let source = gcx.hir.source(id);
        m.serialize_entry(
            "nodes",
            &cx.w(Items {
                ast: &ast.items.raw,
                hir: source.items,
                usings: source.usings,
                imports: source.imports,
            }),
        )?;
        m.end()
    }
}

/// Returns the SPDX license identifier of a source, from its leading comments, and where its first
/// token starts.
fn license_and_start(gcx: Gcx<'_>, file: &SourceFile) -> (Option<String>, BytePos) {
    let mut license = None;
    let mut tokens = Lexer::from_source_file(gcx.sess, file);
    let start = loop {
        let Some(token) = tokens.next() else { break file.end_position() };
        let Some((_, _, comment)) = token.comment() else { break token.span.lo() };
        if license.is_none()
            && let Some((_, rest)) = comment.as_str().split_once("SPDX-License-Identifier:")
        {
            license = Some(rest.lines().next().unwrap_or_default().trim().to_string());
        }
    };
    (license, start)
}

/// The writer of a source: the source being written and what its nodes need to refer to other
/// nodes.
#[derive(Clone, Copy)]
struct Cx<'a, 'gcx> {
    json: &'a AstJson<'gcx>,
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
        s.collect_str(&format_args!(
            "{lo}:{}:{}",
            self.span.hi().to_u32() - self.span.lo().to_u32(),
            self.source.index()
        ))
    }
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
    ) -> Result<(S::SerializeMap, usize), S::Error> {
        let id = self.json.fresh_id();
        Ok((self.node_with_id(s, id, node_type, span)?, id))
    }

    /// Starts a node with the given ID.
    fn node_with_id<S: Serializer>(
        self,
        s: S,
        id: usize,
        node_type: &str,
        span: Span,
    ) -> Result<S::SerializeMap, S::Error> {
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("id", &id)?;
        m.serialize_entry("src", &self.src(span))?;
        m.serialize_entry("nodeType", node_type)?;
        Ok(m)
    }

    /// Returns the span of the first token after `span`, which is empty if there is none.
    fn token_after(&self, span: Span) -> Span {
        let file = &self.gcx.hir.source(self.source).file;
        let rest = &file.src[(span.hi() - file.start_pos).to_usize()..];
        let mut tokens = Lexer::with_start_pos(self.gcx.sess, rest, span.hi());
        tokens
            .find(|token| !token.is_comment_or_doc())
            .map_or(span.shrink_to_hi(), |token| token.span)
    }

    /// Writes the `name` and `nameLocation` of a declaration.
    fn serialize_name<M: SerializeMap>(
        self,
        m: &mut M,
        name: impl Into<Option<Ident>>,
    ) -> Result<(), M::Error> {
        let name = name.into();
        m.serialize_entry("name", &self::name(name))?;
        m.serialize_entry("nameLocation", &self.src(name.map_or(Span::DUMMY, |name| name.span)))
    }

    /// Starts the node of an item.
    fn item_node<S: Serializer>(
        self,
        s: S,
        id: impl Into<ItemId>,
        node_type: &str,
        span: Span,
    ) -> Result<S::SerializeMap, S::Error> {
        self.node_with_id(s, self.item_id(id), node_type, span)
    }

    /// Returns the ID of the import directive that declares the module alias `alias` of `source`.
    ///
    /// The declaration may come from another source that this one imports.
    fn import_id(self, alias: Symbol, source: SourceId) -> Option<usize> {
        let hir = &self.gcx.hir;
        let mut sources =
            std::iter::once(self.source).chain(hir.source_ids().filter(|&id| id != self.source));
        sources.find_map(|id| {
            let ast = self.gcx.sources[id].ast.as_ref()?;
            let index = hir.source(id).imports.iter().position(|&(item, imported)| {
                let ast::ItemKind::Import(import) = &ast.items[item].kind else { return false };
                imported == source && import.source_alias().is_some_and(|ident| ident.name == alias)
            })?;
            Some(self.json.first_imports[id] + index)
        })
    }

    /// Returns the source text of a span.
    fn text(&self, span: Span) -> &str {
        let file = &self.gcx.hir.source(self.source).file;
        &file.src[(span.lo() - file.start_pos).to_usize()..(span.hi() - file.start_pos).to_usize()]
    }

    fn item_id(self, id: impl Into<ItemId>) -> usize {
        self.gcx.hir.global_item_id(id)
    }

    /// Returns the ID of the scope of an item: its contract or its source unit.
    fn scope(self, contract: Option<ContractId>) -> usize {
        contract.map_or_else(|| self.json.source_unit_id(self.source), |id| self.item_id(id))
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

/// The `documentation` of a node, if it has any.
///
/// Like solc, this is the last group of doc comments, which a blank line or another comment ends.
fn serialize_docs<M: SerializeMap>(
    m: &mut M,
    cx: Cx<'_, '_>,
    docs: &DocComments<'_>,
) -> Result<(), M::Error> {
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
        return Ok(());
    }
    let span = Span::join_first_last(docs.iter().map(|doc| doc.span));
    m.serialize_entry("documentation", &cx.w(Doc { span, text }))
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
                text.push_str(if i == 0 { symbol.trim_start() } else { symbol });
            }
            CommentKind::Block => {
                let lines = symbol.split('\n').collect::<Vec<_>>();
                for (i, line) in lines.iter().enumerate() {
                    let mut line = line.trim_start().trim_end_matches('\r');
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
                    hir::Res::Namespace(source) => cx.import_id(name, source),
                    hir::Res::Builtin(_) | hir::Res::Err(_) => None,
                })
                .collect::<Vec<_>>();
            m.serialize_entry(&Sym(name), &ids)?;
        }
        m.end()
    }
}

/// The items of a source or a contract.
struct Items<'a, 'ast> {
    ast: &'a [ast::Item<'ast>],
    /// The HIR items, which also hold the getters and Yul functions lowering adds.
    hir: &'a [ItemId],
    usings: &'a [hir::UsingDirective<'a>],
    /// The imports of the source.
    imports: &'a [(ast::ItemId, SourceId)],
}

impl Serialize for W<'_, '_, Items<'_, '_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, items) = self;
        let hir = &cx.gcx.hir;
        let mut hir_items = items.hir.iter().copied().filter(|&id| match id {
            ItemId::Function(id) => !hir.function(id).is_yul && hir.function(id).gettee.is_none(),
            _ => true,
        });
        let mut usings = items.usings.iter();
        let mut imports = items.imports.iter().enumerate();
        macro_rules! next_hir {
            ($kind:ident) => {
                match hir_items.next() {
                    Some(ItemId::$kind(id)) => id,
                    id => unreachable!("lowering does not match the AST: {id:?}"),
                }
            };
        }

        let mut seq = s.serialize_seq(Some(items.ast.len()))?;
        for (index, item) in items.ast.iter().enumerate() {
            match &item.kind {
                ast::ItemKind::Pragma(pragma) => seq.serialize_element(&cx.w((item, pragma)))?,
                ast::ItemKind::Import(import) => {
                    let (position, &(ast_id, source)) = imports.next().unwrap();
                    debug_assert_eq!(ast_id.index(), index);
                    let id = cx.json.first_imports[cx.source] + position;
                    seq.serialize_element(&cx.w(Import { item, import, id, source }))?;
                }
                ast::ItemKind::Using(using) => {
                    seq.serialize_element(&cx.w((item, using, usings.next().unwrap())))?
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
                    let docs = hir.variable(id).is_state_variable().then_some(&item.docs);
                    let scope = cx.scope(hir.variable(id).contract);
                    seq.serialize_element(&cx.w(Var { ast: variable, id, docs, scope }))?
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
        let (mut m, _) = cx.node(s, "PragmaDirective", item.span)?;
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
        m.serialize_entry("literals", &literals)?;
        m.end()
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
        let mut m = cx.node_with_id(s, *id, "ImportDirective", item.span)?;
        m.serialize_entry("file", &Sym(import.path.value))?;
        m.serialize_entry("sourceUnit", &json.source_unit_id(*source))?;
        m.serialize_entry("scope", &json.source_unit_id(cx.source))?;
        let file_name = &cx.gcx.hir.source(*source).file.name;
        m.serialize_entry("absolutePath", &format_args!("{}", file_name.display()))?;
        let alias = import.source_alias();
        m.serialize_entry("unitAlias", &name(alias))?;
        m.serialize_entry("nameLocation", &cx.src(alias.map_or(Span::DUMMY, |alias| alias.span)))?;
        let aliases = match &import.items {
            ast::ImportItems::Aliases(aliases) => &aliases[..],
            _ => &[],
        };
        let aliases = Seq(|| {
            aliases.iter().map(|&(foreign, local)| {
                cx.w(SymbolAlias { foreign, local, source: *source, name: alias })
            })
        });
        m.serialize_entry("symbolAliases", &aliases)?;
        m.end()
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
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("foreign", &cx.w((*foreign, *source)))?;
        if let Some(local) = local {
            m.serialize_entry("local", &Sym(local.name))?;
        }
        // solc reports the location of the import's own name, not of the symbol's.
        m.serialize_entry("nameLocation", &cx.src(name.map_or(Span::DUMMY, |name| name.span)))?;
        m.end()
    }
}

/// The `Identifier` node of an imported symbol.
impl Serialize for W<'_, '_, (Ident, SourceId)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (foreign, imported)) = self;
        let (mut m, _) = cx.node(s, "Identifier", foreign.span)?;
        m.serialize_entry("name", foreign.as_str())?;
        m.serialize_entry("overloadedDeclarations", &[(); 0])?;
        let declaration = cx.gcx.symbol_resolver.source_scopes[*imported]
            .resolve(*foreign)
            .and_then(|declarations| match declarations {
                [declaration] => Some(declaration),
                _ => None,
            })
            .and_then(|declaration| match declaration.res {
                hir::Res::Item(id) => Some(cx.item_id(id)),
                _ => None,
            });
        if let Some(declaration) = declaration {
            m.serialize_entry("referencedDeclaration", &declaration)?;
        }
        m.serialize_entry("typeDescriptions", &Empty)?;
        m.end()
    }
}

/// An empty JSON object.
struct Empty;

impl Serialize for Empty {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_map(Some(0))?.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::UsingDirective<'_>, &hir::UsingDirective<'_>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, using, hir)) = self;
        let (mut m, _) = cx.node(s, "UsingForDirective", item.span)?;
        if let (Some(ty), Some(hir_ty)) = (&using.ty, &hir.ty) {
            m.serialize_entry("typeName", &cx.w((ty, hir_ty)))?;
        }
        match &using.list {
            ast::UsingList::Single(path) => {
                let library = match hir.entries[0].kind {
                    hir::UsingEntryKind::Library(id) => Some(cx.item_id(id)),
                    _ => None,
                };
                m.serialize_entry("libraryName", &cx.w(IdentPath { path, declaration: library }))?;
            }
            ast::UsingList::Multiple(paths) => {
                let list = Seq(|| {
                    paths.iter().zip(hir.entries).map(|((path, operator), entry)| {
                        let function = match entry.kind {
                            hir::UsingEntryKind::Functions(&[id]) => Some(cx.item_id(id)),
                            _ => None,
                        };
                        cx.w(UsingFunction { path, function, operator: *operator })
                    })
                });
                m.serialize_entry("functionList", &list)?;
            }
        }
        m.serialize_entry("global", &using.global)?;
        m.end()
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
        let mut m = s.serialize_map(None)?;
        let path = cx.w(IdentPath { path, declaration: *function });
        match operator {
            None => m.serialize_entry("function", &path)?,
            Some(operator) => {
                m.serialize_entry("definition", &path)?;
                m.serialize_entry("operator", operator.to_str())?;
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
        let (mut m, _) = cx.node(s, "IdentifierPath", path.span())?;
        m.serialize_entry("name", &format_args!("{path}"))?;
        let locations = Seq(|| path.segments().iter().map(|segment| cx.src(segment.span)));
        m.serialize_entry("nameLocations", &locations)?;
        if let Some(declaration) = declaration {
            m.serialize_entry("referencedDeclaration", declaration)?;
        }
        m.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemContract<'_>, ContractId)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, contract, id)) = self;
        let gcx = cx.gcx;
        let c = gcx.hir.contract(*id);
        let mut m = cx.item_node(s, *id, "ContractDefinition", item.span)?;
        serialize_docs(&mut m, *cx, &item.docs)?;
        cx.serialize_name(&mut m, c.name)?;
        m.serialize_entry(
            "contractKind",
            match c.kind {
                ast::ContractKind::Interface => "interface",
                ast::ContractKind::Library => "library",
                ast::ContractKind::Contract | ast::ContractKind::AbstractContract => "contract",
            },
        )?;
        m.serialize_entry("abstract", &c.is_abstract())?;
        let bases = Seq(|| contract.bases.iter().zip(c.bases).map(|(base, &id)| cx.w((base, id))));
        m.serialize_entry("baseContracts", &bases)?;
        m.serialize_entry("canonicalName", c.name.as_str())?;
        m.serialize_entry(
            "contractDependencies",
            &sorted_ids(*cx, gcx.contract_bytecode_dependencies(*id).iter()),
        )?;
        m.serialize_entry("usedEvents", &sorted_ids(*cx, gcx.interface_events(*id).iter()))?;
        m.serialize_entry("usedErrors", &sorted_ids(*cx, gcx.interface_errors(*id).iter()))?;
        let items = Items { ast: contract.body, hir: c.items, usings: c.usings, imports: &[] };
        m.serialize_entry("nodes", &cx.w(items))?;
        m.serialize_entry("scope", &cx.scope(None))?;
        m.serialize_entry("fullyImplemented", &is_fully_implemented(gcx, *id))?;
        if !c.linearization_failed() {
            let linearized = c.linearized_bases.iter().map(|&base| cx.item_id(base));
            m.serialize_entry("linearizedBaseContracts", &linearized.collect::<Vec<_>>())?;
        }
        m.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Modifier<'_>, ContractId)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (base, id)) = self;
        let (mut m, _) = cx.node(s, "InheritanceSpecifier", base.span())?;
        let declaration = Some(cx.item_id(*id));
        m.serialize_entry("baseName", &cx.w(IdentPath { path: &base.name, declaration }))?;
        m.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemFunction<'_>, hir::FunctionId)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, function, id)) = self;
        let gcx = cx.gcx;
        let f = gcx.hir.function(*id);
        let header = &function.header;
        let item_id = cx.item_id(*id);
        let is_modifier = f.kind.is_modifier();
        let node_type = if is_modifier { "ModifierDefinition" } else { "FunctionDefinition" };
        let mut m = cx.node_with_id(s, item_id, node_type, item.span)?;
        serialize_docs(&mut m, *cx, &item.docs)?;
        cx.serialize_name(&mut m, f.name)?;
        if !is_modifier {
            let kind = if f.is_free() { "freeFunction" } else { f.kind.to_str() };
            m.serialize_entry("kind", kind)?;
            m.serialize_entry("stateMutability", f.state_mutability.to_str())?;
        }
        m.serialize_entry("virtual", &f.marked_virtual)?;
        if let Some(overrides) = &header.override_ {
            m.serialize_entry("overrides", &cx.w((overrides, f.overrides)))?;
        }
        // A modifier without `()` has an empty list at the token after its name.
        let span = match f.name {
            Some(name) if header.parameters.span.is_dummy() => {
                cx.token_after(name.span).shrink_to_lo()
            }
            _ => header.parameters.span,
        };
        let parameters =
            Params { span, ast: header.parameters.vars, hir: f.parameters, scope: item_id };
        m.serialize_entry("parameters", &cx.w(parameters))?;
        if !is_modifier {
            // The empty list of a function without `returns` sits where its body starts.
            let span =
                header.returns.as_ref().map_or(function.body_span.shrink_to_lo(), |r| r.span);
            let returns = Params { span, ast: header.returns(), hir: f.returns, scope: item_id };
            m.serialize_entry("returnParameters", &cx.w(returns))?;
            let modifiers =
                Seq(|| header.modifiers.iter().zip(f.modifiers).map(|(m, hir)| cx.w((m, hir))));
            m.serialize_entry("modifiers", &modifiers)?;
            m.serialize_entry("implemented", &function.is_implemented())?;
            m.serialize_entry("scope", &cx.scope(f.contract))?;
        }
        // A constructor of an abstract contract can only be called by derived contracts.
        let visibility = match f.contract {
            Some(c) if f.kind.is_constructor() => {
                if gcx.hir.contract(c).is_abstract() {
                    "internal"
                } else {
                    "public"
                }
            }
            _ => f.visibility.to_str(),
        };
        m.serialize_entry("visibility", visibility)?;
        if f.is_part_of_external_interface() {
            m.serialize_entry("functionSelector", &hex::encode(gcx.function_selector(*id)))?;
        }
        let bases = gcx.base_override_items((*id).into());
        if !bases.is_empty() {
            let key = if is_modifier { "baseModifiers" } else { "baseFunctions" };
            m.serialize_entry(key, &sorted_ids(*cx, bases.iter().copied()))?;
        }
        m.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Modifier<'_>, &hir::Modifier<'_>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (modifier, hir)) = self;
        let (mut m, _) = cx.node(s, "ModifierInvocation", modifier.span())?;
        let declaration = Some(cx.item_id(hir.id));
        m.serialize_entry("modifierName", &cx.w(IdentPath { path: &modifier.name, declaration }))?;
        let kind = match hir.id {
            ItemId::Contract(_) => "baseConstructorSpecifier",
            _ => "modifierInvocation",
        };
        m.serialize_entry("kind", kind)?;
        m.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Override<'_>, &[ContractId])> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (overrides, ids)) = self;
        let (mut m, _) = cx.node(s, "OverrideSpecifier", overrides.span)?;
        let paths = Seq(|| {
            overrides
                .paths
                .iter()
                .zip(*ids)
                .map(|(path, &id)| cx.w(IdentPath { path, declaration: Some(cx.item_id(id)) }))
        });
        m.serialize_entry("overrides", &paths)?;
        m.end()
    }
}

/// A list of parameters: a `ParameterList` node.
struct Params<'a, 'ast> {
    span: Span,
    ast: &'a [ast::VariableDefinition<'ast>],
    hir: &'a [VariableId],
    /// The ID of the node that declares the parameters.
    scope: usize,
}

impl Serialize for W<'_, '_, Params<'_, '_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Params { span, ast, hir, scope }) = self;
        let (mut m, _) = cx.node(s, "ParameterList", *span)?;
        m.serialize_entry("parameters", &cx.w(Vars { ast, hir, scope: *scope }))?;
        m.end()
    }
}

/// The variables of a parameter list, a struct or an event.
struct Vars<'a, 'ast> {
    ast: &'a [ast::VariableDefinition<'ast>],
    hir: &'a [VariableId],
    scope: usize,
}

impl Serialize for W<'_, '_, Vars<'_, '_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Vars { ast, hir, scope }) = self;
        s.collect_seq(
            ast.iter().zip(*hir).map(|(ast, &id)| cx.w(Var { ast, id, docs: None, scope: *scope })),
        )
    }
}

/// A variable of any kind.
struct Var<'a, 'ast> {
    ast: &'a ast::VariableDefinition<'ast>,
    id: VariableId,
    /// The documentation, for state variables.
    docs: Option<&'a DocComments<'ast>>,
    /// The ID of the node that declares the variable.
    scope: usize,
}

impl Serialize for W<'_, '_, Var<'_, '_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Var { ast, id, docs, scope }) = self;
        let gcx = cx.gcx;
        let var = gcx.hir.variable(*id);
        // solc leaves the `;` out of the location.
        let text = cx.text(ast.span);
        let span = match text.strip_suffix(';') {
            Some(text_without_semi) => {
                ast.span.with_hi(ast.span.lo() + text_without_semi.trim_end().len() as u32)
            }
            None => ast.span,
        };
        // The end of a function type is the end of the next token, which an unnamed variable
        // of the type shares.
        let span = match ast.ty.kind {
            ast::TypeKind::Function(_) if var.name.is_none() => {
                span.with_hi(cx.token_after(ast.ty.span).hi())
            }
            _ => span,
        };
        let mut m = cx.item_node(s, *id, "VariableDeclaration", span)?;
        if let Some(docs) = docs {
            serialize_docs(&mut m, *cx, docs)?;
        }
        cx.serialize_name(&mut m, var.name)?;
        m.serialize_entry("typeName", &cx.w((&ast.ty, &var.ty)))?;
        m.serialize_entry("constant", &var.is_constant())?;
        let mutability = match var.mutability {
            None => "mutable",
            Some(mutability) => mutability.to_str(),
        };
        m.serialize_entry("mutability", mutability)?;
        m.serialize_entry("stateVariable", &var.is_state_variable())?;
        m.serialize_entry(
            "storageLocation",
            var.data_location.map_or("default", |loc| loc.to_str()),
        )?;
        if let Some(overrides) = &ast.override_ {
            m.serialize_entry("overrides", &cx.w((overrides, var.overrides)))?;
        }
        m.serialize_entry(
            "visibility",
            var.visibility.unwrap_or(ast::Visibility::Internal).to_str(),
        )?;
        m.serialize_entry("scope", scope)?;
        // solc marks the reference types of every variable but a storage state variable as
        // pointers.
        let pointer = !var.is_state_variable() || var.mutability.is_some();
        let ty = gcx.type_of_item((*id).into());
        m.serialize_entry("typeDescriptions", &ty::TypeDescriptions { gcx, ty, pointer })?;
        if let Some(getter) = var.getter {
            m.serialize_entry("functionSelector", &hex::encode(gcx.function_selector(getter)))?;
            let bases = gcx.base_override_items((*id).into());
            if !bases.is_empty() {
                m.serialize_entry("baseFunctions", &sorted_ids(*cx, bases.iter().copied()))?;
            }
        }
        if cx.in_event {
            m.serialize_entry("indexed", &var.indexed)?;
        }
        m.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemStruct<'_>, hir::StructId)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, strukt, id)) = self;
        let gcx = cx.gcx;
        let hir = gcx.hir.strukt(*id);
        let item_id = cx.item_id(*id);
        let mut m = cx.node_with_id(s, item_id, "StructDefinition", item.span)?;
        serialize_docs(&mut m, *cx, &item.docs)?;
        cx.serialize_name(&mut m, hir.name)?;
        m.serialize_entry("visibility", "public")?;
        m.serialize_entry(
            "members",
            &cx.w(Vars { ast: strukt.fields, hir: hir.fields, scope: item_id }),
        )?;
        m.serialize_entry("scope", &cx.scope(hir.contract))?;
        m.serialize_entry("canonicalName", &format_args!("{}", gcx.item_canonical_name(*id)))?;
        m.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemEnum<'_>, hir::EnumId)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, _, id)) = self;
        let gcx = cx.gcx;
        let hir = gcx.hir.enumm(*id);
        let mut m = cx.item_node(s, *id, "EnumDefinition", item.span)?;
        serialize_docs(&mut m, *cx, &item.docs)?;
        cx.serialize_name(&mut m, hir.name)?;
        m.serialize_entry("members", &cx.w(EnumValues(hir.variants)))?;
        m.serialize_entry("canonicalName", &format_args!("{}", gcx.item_canonical_name(*id)))?;
        m.end()
    }
}

/// The variants of an enum: `EnumValue` nodes.
struct EnumValues<'a>(&'a [VariableId]);

impl Serialize for W<'_, '_, EnumValues<'_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, EnumValues(variants)) = self;
        let mut seq = s.serialize_seq(Some(variants.len()))?;
        for &id in *variants {
            let variant = cx.gcx.hir.variable(id).name.unwrap();
            seq.serialize_element(&cx.w((id, variant)))?;
        }
        seq.end()
    }
}

impl Serialize for W<'_, '_, (VariableId, Ident)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (id, name)) = self;
        let mut m = cx.item_node(s, *id, "EnumValue", name.span)?;
        cx.serialize_name(&mut m, *name)?;
        m.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemUdvt<'_>, hir::UdvtId)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, udvt, id)) = self;
        let gcx = cx.gcx;
        let hir = gcx.hir.udvt(*id);
        let mut m = cx.item_node(s, *id, "UserDefinedValueTypeDefinition", item.span)?;
        cx.serialize_name(&mut m, hir.name)?;
        m.serialize_entry("underlyingType", &cx.w((&udvt.ty, &hir.ty)))?;
        m.serialize_entry("canonicalName", &format_args!("{}", gcx.item_canonical_name(*id)))?;
        m.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemEvent<'_>, hir::EventId)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, event, id)) = self;
        let gcx = cx.gcx;
        let hir = gcx.hir.event(*id);
        let item_id = cx.item_id(*id);
        let mut m = cx.node_with_id(s, item_id, "EventDefinition", item.span)?;
        serialize_docs(&mut m, *cx, &item.docs)?;
        cx.serialize_name(&mut m, hir.name)?;
        let parameters = Params {
            span: event.parameters.span,
            ast: event.parameters.vars,
            hir: hir.parameters,
            scope: item_id,
        };
        m.serialize_entry("parameters", &Cx { in_event: true, ..*cx }.w(parameters))?;
        m.serialize_entry("anonymous", &hir.anonymous)?;
        m.serialize_entry("eventSelector", &hex::encode(gcx.event_selector(*id)))?;
        m.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Item<'_>, &ast::ItemError<'_>, hir::ErrorId)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (item, error, id)) = self;
        let gcx = cx.gcx;
        let hir = gcx.hir.error(*id);
        let item_id = cx.item_id(*id);
        let mut m = cx.node_with_id(s, item_id, "ErrorDefinition", item.span)?;
        serialize_docs(&mut m, *cx, &item.docs)?;
        cx.serialize_name(&mut m, hir.name)?;
        let parameters = Params {
            span: error.parameters.span,
            ast: error.parameters.vars,
            hir: hir.parameters,
            scope: item_id,
        };
        m.serialize_entry("parameters", &cx.w(parameters))?;
        m.serialize_entry("errorSelector", &hex::encode(gcx.function_selector(*id)))?;
        m.end()
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
        let end = cx.gcx.hir.source(cx.source).file.end_position();
        let rest = cx.text(span.shrink_to_hi().with_hi(end));
        if cx.text(span).starts_with("//") {
            let line_break =
                if rest.starts_with("\r\n") { 2 } else { rest.starts_with(['\r', '\n']) as usize };
            if !rest[line_break..].starts_with([' ', '\t']) {
                span = span.with_hi(span.hi() + line_break as u32);
            }
        }
        let (mut m, _) = cx.node(s, "StructuredDocumentation", span)?;
        m.serialize_entry("text", text)?;
        m.end()
    }
}
