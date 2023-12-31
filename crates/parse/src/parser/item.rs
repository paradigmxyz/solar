use crate::{PResult, Parser};
use sulk_ast::{ast::*, token::*};
use sulk_interface::{kw, sym};

use super::SeqSep;

impl<'a> Parser<'a> {
    /// Parses a source unit.
    pub fn parse_file(&mut self) -> PResult<'a, SourceUnit> {
        let mut items = Vec::new();
        while let Some(item) = self.parse_item()? {
            items.push(item);
        }
        if !self.eat(&TokenKind::Eof) {
            let msg = format!("expected item, found {}", self.token.full_description());
            return Err(self.dcx().err(msg).span(self.token.span));
        }
        Ok(SourceUnit { items })
    }

    /// Parses an item.
    pub fn parse_item(&mut self) -> PResult<'a, Option<Item>> {
        let lo = self.token.span;
        let kind = self.parse_item_kind()?;
        Ok(kind.map(|kind| {
            let span = lo.to(self.prev_token.span);
            Item { span, kind }
        }))
    }

    fn parse_item_kind(&mut self) -> PResult<'a, Option<ItemKind>> {
        let kind = if self.is_function_like() {
            self.parse_function().map(ItemKind::Function)
        } else if self.eat_keyword(kw::Struct) {
            self.parse_struct().map(ItemKind::Struct)
        } else if self.eat_keyword(kw::Event) {
            self.parse_event().map(ItemKind::Event)
        } else if self.is_contract_like() {
            self.parse_contract().map(ItemKind::Contract)
        } else if self.eat_keyword(kw::Enum) {
            self.parse_enum().map(ItemKind::Enum)
        } else if self.eat_keyword(kw::Type) {
            self.parse_udvt().map(ItemKind::Udvt)
        } else if self.eat_keyword(kw::Pragma) {
            self.parse_pragma().map(ItemKind::Pragma)
        } else if self.eat_keyword(kw::Import) {
            self.parse_import().map(ItemKind::Import)
        } else if self.eat_keyword(kw::Using) {
            self.parse_using().map(ItemKind::Using)
        } else if self.eat_keyword(sym::error)
            && self.look_ahead(1).is_ident()
            && self.look_ahead(2).is_open_delim(Delimiter::Parenthesis)
        {
            self.parse_error().map(ItemKind::Error)
        } else if self.is_variable_declaration() {
            self.parse_variable_definition().map(ItemKind::Variable)
        } else {
            return Ok(None);
        };
        kind.map(Some)
    }

    /// Returns `true` if the current token is the start of a function definition.
    fn is_function_like(&self) -> bool {
        (self.token.is_keyword(kw::Function)
            && !self.look_ahead(1).is_open_delim(Delimiter::Parenthesis))
            || self.token.is_keyword_any(&[
                kw::Constructor,
                kw::Fallback,
                kw::Receive,
                kw::Modifier,
            ])
    }

    /// Returns `true` if the current token is the start of a contract definition.
    fn is_contract_like(&self) -> bool {
        self.token.is_keyword_any(&[kw::Abstract, kw::Contract, kw::Interface, kw::Library])
    }

    /// Returns `true` if the current token is the start of a variable declaration.
    fn is_variable_declaration(&self) -> bool {
        // https://github.com/ethereum/solidity/blob/194b114664c7daebc2ff68af3c573272f5d28913/libsolidity/parsing/Parser.cpp#L2451
        self.token.is_non_reserved_ident(false)
            || self.token.is_keyword(kw::Mapping)
            || (self.token.is_keyword(kw::Function)
                && self.look_ahead(1).is_open_delim(Delimiter::Parenthesis))
            || self.token.is_elementary_type()
    }

    /* ----------------------------------------- Items ----------------------------------------- */
    // These functions expect that the keyword has already been eaten unless otherwise noted.

    /// Parses a function definition.
    fn parse_function(&mut self) -> PResult<'a, ItemFunction> {
        let TokenKind::Ident(kw) = self.token.kind else {
            unreachable!("parse_function called without function-like keyword");
        };
        let kind = match kw {
            kw::Constructor => FunctionKind::Constructor,
            kw::Function => FunctionKind::Function,
            kw::Fallback => FunctionKind::Fallback,
            kw::Receive => FunctionKind::Receive,
            kw::Modifier => FunctionKind::Modifier,
            _ => unreachable!("parse_function called without function-like keyword"),
        };
        let name = if kind.requires_name() { Some(self.parse_ident()?) } else { None };
        let parameters =
            if kind.can_omit_parens() && !self.token.is_open_delim(Delimiter::Parenthesis) {
                Vec::new()
            } else {
                self.parse_parameter_list(VarDeclMode::OnlyStorage)?
            };
        let attributes = self.parse_function_attributes()?;
        if !kind.can_have_attributes() && !attributes.is_empty() {
            let msg = format!("{kind}s cannot have attributes");
            self.dcx().err(msg).span(attributes.span).emit();
        }
        let returns = if self.eat_keyword(kw::Returns) {
            self.parse_parameter_list(VarDeclMode::OnlyStorage)?
        } else {
            Vec::new()
        };
        let body = if self.eat(&TokenKind::Semi) { None } else { Some(self.parse_block()?) };
        Ok(ItemFunction { kind, name, parameters, attributes, returns, body })
    }

    /// Parses a function's attributes.
    fn parse_function_attributes(&mut self) -> PResult<'a, FunctionAttributes> {
        let lo = self.token.span;
        let visibility = self.parse_visibility();
        let state_mutability = self.parse_state_mutability();
        let modifiers = self.parse_modifiers()?;
        let virtual_ = self.eat_keyword(kw::Virtual);
        let overrides = self.parse_overrides()?;
        let span = lo.to(self.prev_token.span);
        Ok(FunctionAttributes {
            span,
            visibility,
            state_mutability,
            modifiers,
            virtual_,
            overrides,
        })
    }

    /// Parses a struct definition.
    fn parse_struct(&mut self) -> PResult<'a, ItemStruct> {
        let name = self.parse_ident()?;
        let (fields, _) = self.parse_delim_seq(Delimiter::Brace, SeqSep::none(), |this| {
            let var = this.parse_variable_declaration(VarDeclMode::None)?;
            this.expect(&TokenKind::Semi)?;
            Ok(var)
        })?;
        Ok(ItemStruct { name, fields })
    }

    /// Parses an event definition.
    fn parse_event(&mut self) -> PResult<'a, ItemEvent> {
        let name = self.parse_ident()?;
        let parameters = self.parse_parameter_list(VarDeclMode::OnlyIndexed)?;
        self.expect(&TokenKind::Semi)?;
        Ok(ItemEvent { name, parameters })
    }

    /// Parses an error definition.
    fn parse_error(&mut self) -> PResult<'a, ItemError> {
        let name = self.parse_ident()?;
        let parameters = self.parse_parameter_list(VarDeclMode::None)?;
        self.expect(&TokenKind::Semi)?;
        Ok(ItemError { name, parameters })
    }

    /// Parses a contract definition.
    ///
    /// Expects the current token to be a contract-like keyword.
    fn parse_contract(&mut self) -> PResult<'a, ItemContract> {
        let TokenKind::Ident(kw) = self.token.kind else {
            unreachable!("parse_contract called without contract-like keyword");
        };
        self.bump();
        let kind = match kw {
            kw::Abstract => {
                self.expect_keyword(kw::Contract)?;
                ContractKind::AbstractContract
            }
            kw::Contract => ContractKind::Contract,
            kw::Interface => ContractKind::Interface,
            kw::Library => ContractKind::Library,
            _ => unreachable!("parse_contract called without contract-like keyword"),
        };
        let name = self.parse_ident()?;
        let inheritance =
            if self.eat_keyword(kw::Is) { self.parse_modifiers()? } else { Vec::new() };
        let body = self.in_contract(|this| this.parse_items())?;
        Ok(ItemContract { kind, name, inheritance, body })
    }

    /// Parses an enum definition.
    fn parse_enum(&mut self) -> PResult<'a, ItemEnum> {
        todo!()
    }

    /// Parses a user-defined value type.
    fn parse_udvt(&mut self) -> PResult<'a, ItemUdvt> {
        self.expect(&TokenKind::Semi)?;
        todo!()
    }

    /// Parses a pragma directive.
    fn parse_pragma(&mut self) -> PResult<'a, PragmaDirective> {
        self.expect(&TokenKind::Semi)?;
        todo!()
    }

    /// Parses an import directive.
    fn parse_import(&mut self) -> PResult<'a, ImportDirective> {
        self.expect(&TokenKind::Semi)?;
        todo!()
    }

    fn parse_using(&mut self) -> PResult<'a, UsingDirective> {
        self.expect(&TokenKind::Semi)?;
        todo!()
    }

    /* ----------------------------------------- Common ----------------------------------------- */

    /// Parses a variable definition.
    pub(super) fn parse_variable_definition(&mut self) -> PResult<'a, VariableDefinition> {
        let ty = self.parse_type()?;
        let storage = self.parse_storage();
        let visibility = self.parse_visibility();
        let mutability = self.parse_variable_mutability();
        let name = self.parse_ident()?;
        let initializer = if self.eat(&TokenKind::Eq) { Some(self.parse_expr()?) } else { None };
        self.expect(&TokenKind::Semi)?;
        Ok(VariableDefinition { ty, storage, visibility, mutability, name, initializer })
    }

    /// Parses mutability of a variable: `constant | immutable`.
    pub(super) fn parse_variable_mutability(&mut self) -> Option<VarMut> {
        if self.eat_keyword(kw::Constant) {
            Some(VarMut::Constant)
        } else if self.eat_keyword(kw::Immutable) {
            Some(VarMut::Immutable)
        } else {
            None
        }
    }

    /// Parses a parameter list: `($(vardecl),*)`.
    pub(super) fn parse_parameter_list(&mut self, opts: VarDeclMode) -> PResult<'a, ParameterList> {
        self.parse_paren_comma_seq(|this| this.parse_variable_declaration(opts)).map(|(x, _)| x)
    }

    /// Parses a variable declaration: `type storage? indexed? name`.
    pub(super) fn parse_variable_declaration(
        &mut self,
        opts: VarDeclMode,
    ) -> PResult<'a, VariableDeclaration> {
        let ty = self.parse_type()?;
        let mut storage = self.parse_storage();
        if opts.no_storage() && storage.is_some() {
            storage = None;
            let msg = "storage specifiers are not allowed here";
            self.dcx().err(msg).span(self.prev_token.span).emit();
        }
        let name = self.parse_ident_opt()?;
        let mut indexed = self.eat_keyword(kw::Indexed);
        if opts.no_indexed() && indexed {
            indexed = false;
            let msg = "`indexed` is not allowed here";
            self.dcx().err(msg).span(self.prev_token.span).emit();
        }
        Ok(VariableDeclaration { ty, storage, name, indexed })
    }

    /// Parses a list of modifier invocations.
    pub(super) fn parse_modifiers(&mut self) -> PResult<'a, Vec<Modifier>> {
        let mut modifiers = Vec::new();
        while let Some(name) = self.parse_ident_maybe_recover(false).map_err(|e| e.cancel()).ok() {
            let name = self.parse_path_with(name)?;
            let arguments =
                if self.look_ahead(1).kind == TokenKind::OpenDelim(Delimiter::Parenthesis) {
                    self.parse_call_args()?
                } else {
                    CallArgs::empty()
                };
            modifiers.push(Modifier { name, arguments });
        }
        Ok(modifiers)
    }

    /// Parses a list of function overrides.
    pub(super) fn parse_overrides(&mut self) -> PResult<'a, Vec<Override>> {
        let mut overrides = Vec::new();
        while self.eat_keyword(kw::Override) {
            overrides.push(self.parse_override()?);
        }
        Ok(overrides)
    }

    /// Parses a single function override.
    ///
    /// Expects the `override` to have already been eaten.
    fn parse_override(&mut self) -> PResult<'a, Override> {
        debug_assert!(self.prev_token.is_keyword(kw::Override));
        let lo = self.prev_token.span;
        let (paths, _) = self.parse_paren_comma_seq(|this| this.parse_path())?;
        let span = lo.to(self.prev_token.span);
        Ok(Override { span, paths })
    }

    /// Parses a storage location: `storage | memory | calldata`.
    pub(super) fn parse_storage(&mut self) -> Option<Storage> {
        if self.eat_keyword(kw::Storage) {
            Some(Storage::Storage)
        } else if self.eat_keyword(kw::Memory) {
            Some(Storage::Memory)
        } else if self.eat_keyword(kw::Calldata) {
            Some(Storage::Calldata)
        } else {
            None
        }
    }

    /// Parses a visibility: `public | private | internal | external`.
    pub(super) fn parse_visibility(&mut self) -> Option<Visibility> {
        if self.eat_keyword(kw::Public) {
            Some(Visibility::Public)
        } else if self.eat_keyword(kw::Private) {
            Some(Visibility::Private)
        } else if self.eat_keyword(kw::Internal) {
            Some(Visibility::Internal)
        } else if self.eat_keyword(kw::External) {
            Some(Visibility::External)
        } else {
            None
        }
    }

    /// Parses state mutability: `payable | pure | view`.
    pub(super) fn parse_state_mutability(&mut self) -> Option<StateMutability> {
        if self.eat_keyword(kw::Payable) {
            Some(StateMutability::Payable)
        } else if self.eat_keyword(kw::Pure) {
            Some(StateMutability::Pure)
        } else if self.eat_keyword(kw::View) {
            Some(StateMutability::View)
        } else {
            None
        }
    }
}

/// Options for parsing variable declarations.
#[derive(Clone, Copy)]
pub(super) enum VarDeclMode {
    OnlyIndexed, // events
    OnlyStorage, // functions
    None,        // structs, errors
}

impl VarDeclMode {
    pub(super) fn no_storage(&self) -> bool {
        !matches!(self, VarDeclMode::OnlyStorage)
    }

    pub(super) fn no_indexed(&self) -> bool {
        !matches!(self, VarDeclMode::OnlyIndexed)
    }
}
