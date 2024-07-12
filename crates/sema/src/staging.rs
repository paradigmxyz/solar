use sulk_ast::{ast, visit::Visit};
use sulk_data_structures::map::FxBuildHasher;
use sulk_interface::{diagnostics::DiagCtxt, Ident, Session, Span, Symbol};

type Scopes = sulk_data_structures::scope::Scopes<Symbol, Span, FxBuildHasher>;
type Scope = sulk_data_structures::scope::Scope<Symbol, Span, FxBuildHasher>;

pub struct SymbolCollector<'sess> {
    scopes: Scopes,
    dcx: &'sess DiagCtxt,
}

impl<'sess> SymbolCollector<'sess> {
    /// Creates a new symbol collector.
    pub fn new(sess: &'sess Session) -> Self {
        let mut scopes = Scopes::default();
        setup_builtin_scope(scopes.current_mut());
        Self { scopes, dcx: &sess.dcx }
    }

    /// Returns the diagnostics context.
    #[inline]
    pub fn dcx(&self) -> &'sess DiagCtxt {
        self.dcx
    }

    fn add(&mut self, ident: Ident) {
        if let Some((idx, &prev_span)) = self.scopes.position(&ident.name) {
            if idx == 0 {
                let msg = "this declaration shadows a builtin symbol";
                self.dcx.warn(msg).span(ident.span).emit();
            } else {
                let msg = format!("the symbol {ident} is already declared in this scope");
                let help = "previous declaration here";
                self.dcx.err(msg).span(ident.span).span_help(prev_span, help).emit();
            }
            return;
        }
        self.scopes.insert(ident.name, ident.span);
    }

    fn in_scope(&mut self, f: impl FnOnce(&mut Self)) {
        self.scopes.enter();
        f(self);
        self.scopes.exit();
    }
}

impl<'ast> Visit<'ast> for SymbolCollector<'_> {
    fn visit_import_directive(&mut self, import: &'ast ast::ImportDirective<'ast>) {
        match import.items {
            ast::ImportItems::Plain(alias) | ast::ImportItems::Glob(alias) => {
                if let Some(alias) = alias {
                    self.add(alias);
                }
            }
            ast::ImportItems::Aliases(ref imports) => {
                for &(import, alias) in imports.iter() {
                    self.add(alias.unwrap_or(import));
                }
            }
        }
    }

    fn visit_item_contract(&mut self, contract: &'ast ast::ItemContract<'ast>) {
        self.add(contract.name);
        self.in_scope(|this| this.walk_item_contract(contract));
    }
}

fn setup_builtin_scope(_scope: &mut Scope) {}
