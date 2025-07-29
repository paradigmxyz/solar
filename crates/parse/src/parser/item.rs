use super::{ExpectedToken, SeqSep};
use crate::{PResult, Parser};
use itertools::Itertools;
use smallvec::SmallVec;
use solar_ast::{token::*, *};
use solar_interface::{Ident, Span, Spanned, diagnostics::DiagMsg, error_code, kw, sym};

impl<'sess, 'ast> Parser<'sess, 'ast> {
    /// Parses a source unit.
    #[instrument(level = "debug", skip_all)]
    pub fn parse_file(&mut self) -> PResult<'sess, SourceUnit<'ast>> {
        self.parse_items(TokenKind::Eof).map(SourceUnit::new)
    }

    /// Parses a list of items until the given token is encountered.
    fn parse_items(&mut self, end: TokenKind) -> PResult<'sess, Box<'ast, [Item<'ast>]>> {
        let get_msg_note = |this: &mut Self| {
            let (prefix, list, link);
            if this.in_contract {
                prefix = "contract";
                list = "function, variable, struct, or modifier definition";
                link = "contractBodyElement";
            } else {
                prefix = "global";
                list = "pragma, import directive, contract, interface, library, struct, enum, constant, function, modifier, or error definition";
                link = "sourceUnit";
            }
            let msg =
                format!("expected {prefix} item ({list}), found {}", this.token.full_description());
            let note = format!(
                "for a full list of valid {prefix} items, see <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.{link}>"
            );
            (msg, note)
        };

        let mut items = Vec::new();
        while let Some(item) = self.parse_item()? {
            if self.in_contract && !item.is_allowed_in_contract() {
                let msg = format!("{}s are not allowed in contracts", item.description());
                let (_, note) = get_msg_note(self);
                self.dcx().err(msg).span(item.span).note(note).emit();
            } else {
                items.push(item);
            }
        }
        if !self.eat(end) {
            let (msg, note) = get_msg_note(self);
            return Err(self.dcx().err(msg).span(self.token.span).note(note));
        }
        Ok(self.alloc_vec(items))
    }

    /// Parses an item.
    #[instrument(level = "debug", skip_all)]
    pub fn parse_item(&mut self) -> PResult<'sess, Option<Item<'ast>>> {
        let docs = self.parse_doc_comments();
        self.parse_spanned(Self::parse_item_kind)
            .map(|(span, kind)| kind.map(|kind| Item { docs, span, kind }))
    }

    fn parse_item_kind(&mut self) -> PResult<'sess, Option<ItemKind<'ast>>> {
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
        } else if self.check_keyword(sym::error)
            && self.look_ahead(1).is_ident()
            && self.look_ahead(2).is_open_delim(Delimiter::Parenthesis)
        {
            self.bump(); // `error`
            self.parse_error().map(ItemKind::Error)
        } else if self.is_variable_declaration() {
            let flags = if self.in_contract { VarFlags::STATE_VAR } else { VarFlags::CONSTANT_VAR };
            self.parse_variable_definition(flags).map(ItemKind::Variable)
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
    pub(super) fn is_variable_declaration(&self) -> bool {
        // https://github.com/ethereum/solidity/blob/194b114664c7daebc2ff68af3c573272f5d28913/libsolidity/parsing/Parser.cpp#L2451
        self.token.is_non_reserved_ident(false) || self.is_non_custom_variable_declaration()
    }

    pub(super) fn is_non_custom_variable_declaration(&self) -> bool {
        self.token.is_keyword(kw::Mapping)
            || (self.token.is_keyword(kw::Function)
                && self.look_ahead(1).is_open_delim(Delimiter::Parenthesis))
            || self.token.is_elementary_type()
    }

    /* ----------------------------------------- Items ----------------------------------------- */
    // These functions expect that the keyword has already been eaten unless otherwise noted.

    /// Parses a function definition.
    ///
    /// Expects the current token to be a function-like keyword.
    fn parse_function(&mut self) -> PResult<'sess, ItemFunction<'ast>> {
        let Token { span: lo, kind: TokenKind::Ident(kw) } = self.token else {
            unreachable!("parse_function called without function-like keyword");
        };
        self.bump(); // kw

        let kind = match kw {
            kw::Constructor => FunctionKind::Constructor,
            kw::Function => FunctionKind::Function,
            kw::Fallback => FunctionKind::Fallback,
            kw::Receive => FunctionKind::Receive,
            kw::Modifier => FunctionKind::Modifier,
            _ => unreachable!("parse_function called without function-like keyword"),
        };
        let flags = FunctionFlags::from_kind(kind);
        let header = self.parse_function_header(flags)?;
        let (body_span, body) = self.parse_spanned(|this| {
            Ok(if !flags.contains(FunctionFlags::ONLY_BLOCK) && this.eat(TokenKind::Semi) {
                None
            } else {
                Some(this.parse_block()?)
            })
        })?;

        if !self.in_contract && !kind.allowed_in_global() {
            let msg = format!("{kind}s are not allowed in the global scope");
            self.dcx().err(msg).span(lo.to(self.prev_token.span)).emit();
        }
        // All function kinds are allowed in contracts.

        Ok(ItemFunction { kind, header, body, body_span })
    }

    /// Parses a function a header.
    pub(super) fn parse_function_header(
        &mut self,
        flags: FunctionFlags,
    ) -> PResult<'sess, FunctionHeader<'ast>> {
        let lo = self.prev_token.span; // the header span includes the "function" kw

        let mut header = FunctionHeader::default();
        let var_flags = if flags.contains(FunctionFlags::PARAM_NAME) {
            VarFlags::FUNCTION_TY
        } else {
            VarFlags::FUNCTION
        };

        if flags.contains(FunctionFlags::NAME) {
            // Allow and warn on `function fallback` or `function receive`.
            let ident;
            if flags == FunctionFlags::FUNCTION
                && self.token.is_keyword_any(&[kw::Fallback, kw::Receive])
            {
                let kw_span = self.prev_token.span;
                ident = self.parse_ident_any()?;
                let msg = format!("function named `{ident}`");
                let mut warn = self.dcx().warn(msg).span(ident.span).code(error_code!(3445));
                if self.in_contract {
                    let help = format!(
                        "remove the `function` keyword if you intend this to be a contract's {ident} function"
                    );
                    warn = warn.span_help(kw_span, help);
                }
                warn.emit();
            } else {
                ident = self.parse_ident()?;
            }
            header.name = Some(ident);
        } else if self.token.is_non_reserved_ident(false) {
            let msg = "function names are not allowed here";
            self.dcx().err(msg).span(self.token.span).emit();
            self.bump();
        }

        if flags.contains(FunctionFlags::NO_PARENS)
            && !self.token.is_open_delim(Delimiter::Parenthesis)
        {
            // Omitted parens.
        } else {
            header.parameters = self.parse_parameter_list(true, var_flags)?;
        }

        let mut modifiers = Vec::new();
        loop {
            // This is needed to skip parsing surrounding variable's visibility in function types.
            // E.g. in `function(uint) external internal e;` the `internal` is the surrounding
            // variable's visibility, not the function's.
            if !(flags == FunctionFlags::FUNCTION_TY && header.visibility.is_some())
                && let Some(visibility) = self.parse_visibility()
            {
                let span = self.prev_token.span;
                if let Some(prev) = header.visibility {
                    let msg = "visibility already specified";
                    self.dcx()
                        .err(msg)
                        .span(span)
                        .span_note(prev.span, "previous definition")
                        .emit();
                } else {
                    let mut v = Some(visibility);
                    if !flags.contains(FunctionFlags::from_visibility(visibility)) {
                        let msg = visibility_error(visibility, flags.visibilities());
                        self.dcx().err(msg).span(span).emit();
                        // Set to the first valid visibility, if any.
                        v = flags.visibilities().into_iter().flatten().next();
                    }
                    header.visibility = v.map(|v| Spanned { span, data: v });
                }
            } else if let Some(state_mutability) = self.parse_state_mutability() {
                let span = self.prev_token.span;
                if let Some(prev) = header.state_mutability {
                    let msg = "state mutability already specified";
                    self.dcx()
                        .err(msg)
                        .span(span)
                        .span_note(prev.span, "previous definition")
                        .emit();
                } else {
                    let mut sm = Some(state_mutability);
                    if !flags.contains(FunctionFlags::from_state_mutability(state_mutability)) {
                        let msg =
                            state_mutability_error(state_mutability, flags.state_mutabilities());
                        self.dcx().err(msg).span(span).emit();
                        // Set to the first valid state mutability, if any.
                        sm = flags.state_mutabilities().into_iter().flatten().next();
                    }
                    header.state_mutability = sm.map(|sm| Spanned { span, data: sm });
                }
            } else if self.eat_keyword(kw::Virtual) {
                let span = self.prev_token.span;
                if !flags.contains(FunctionFlags::VIRTUAL) {
                    let msg = "`virtual` is not allowed here";
                    self.dcx().err(msg).span(span).emit();
                } else if let Some(prev) = header.virtual_ {
                    let msg = "virtual already specified";
                    self.dcx().err(msg).span(span).span_note(prev, "previous definition").emit();
                } else {
                    header.virtual_ = Some(span);
                }
            } else if self.eat_keyword(kw::Override) {
                let o = self.parse_override()?;
                let span = o.span;
                if !flags.contains(FunctionFlags::OVERRIDE) {
                    let msg = "`override` is not allowed here";
                    self.dcx().err(msg).span(span).emit();
                } else if let Some(prev) = &header.override_ {
                    let msg = "override already specified";
                    self.dcx()
                        .err(msg)
                        .span(span)
                        .span_note(prev.span, "previous definition")
                        .emit();
                } else {
                    header.override_ = Some(o);
                }
            } else if flags.contains(FunctionFlags::MODIFIERS)
                && self.token.is_non_reserved_ident(false)
            {
                modifiers.push(self.parse_modifier()?);
            } else {
                break;
            }
        }

        header.modifiers = self.alloc_vec(modifiers);

        if flags.contains(FunctionFlags::RETURNS) && self.eat_keyword(kw::Returns) {
            header.returns = Some(self.parse_parameter_list(false, var_flags)?);
        }

        header.span = lo.to(self.prev_token.span);

        Ok(header)
    }

    /// Parses a struct definition.
    fn parse_struct(&mut self) -> PResult<'sess, ItemStruct<'ast>> {
        let name = self.parse_ident()?;
        let fields = self.parse_delim_seq(
            Delimiter::Brace,
            SeqSep::trailing_enforced(TokenKind::Semi),
            true,
            |this| this.parse_variable_definition(VarFlags::STRUCT),
        )?;
        Ok(ItemStruct { name, fields })
    }

    /// Parses an event definition.
    fn parse_event(&mut self) -> PResult<'sess, ItemEvent<'ast>> {
        let name = self.parse_ident()?;
        let parameters = self.parse_parameter_list(true, VarFlags::EVENT)?;
        let anonymous = self.eat_keyword(kw::Anonymous);
        self.expect_semi()?;
        Ok(ItemEvent { name, parameters, anonymous })
    }

    /// Parses an error definition.
    fn parse_error(&mut self) -> PResult<'sess, ItemError<'ast>> {
        let name = self.parse_ident()?;
        let parameters = self.parse_parameter_list(true, VarFlags::ERROR)?;
        self.expect_semi()?;
        Ok(ItemError { name, parameters })
    }

    /// Parses a contract definition.
    ///
    /// Expects the current token to be a contract-like keyword.
    fn parse_contract(&mut self) -> PResult<'sess, ItemContract<'ast>> {
        let TokenKind::Ident(kw) = self.token.kind else {
            unreachable!("parse_contract called without contract-like keyword");
        };
        self.bump(); // kw

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

        let mut bases = None::<Box<'_, [Modifier<'_>]>>;
        let mut layout = None::<StorageLayoutSpecifier<'_>>;
        loop {
            if self.eat_keyword(kw::Is) {
                let new_bases = self.parse_inheritance()?;
                if let Some(prev) = &bases {
                    let msg = "base contracts already specified";
                    let span = |bases: &[Modifier<'_>]| {
                        Span::join_first_last(bases.iter().map(|m| m.span()))
                    };
                    self.dcx()
                        .err(msg)
                        .span(span(new_bases))
                        .span_note(span(prev), "previous definition")
                        .emit();
                } else if !new_bases.is_empty() {
                    bases = Some(new_bases);
                }
            } else if self.check_keyword(sym::layout) {
                let new_layout = self.parse_storage_layout_specifier()?;
                if let Some(prev) = &layout {
                    let msg = "storage layout already specified";
                    self.dcx()
                        .err(msg)
                        .span(new_layout.span)
                        .span_note(prev.span, "previous definition")
                        .emit();
                } else {
                    layout = Some(new_layout);
                }
            } else {
                break;
            }
        }

        if let Some(layout) = &layout
            && !kind.is_contract()
        {
            let msg = "storage layout is only allowed for contracts";
            self.dcx().err(msg).span(layout.span).emit();
        }

        self.expect(TokenKind::OpenDelim(Delimiter::Brace))?;
        let body =
            self.in_contract(|this| this.parse_items(TokenKind::CloseDelim(Delimiter::Brace)))?;

        Ok(ItemContract { kind, name, layout, bases: bases.unwrap_or_default(), body })
    }

    /// Parses an enum definition.
    fn parse_enum(&mut self) -> PResult<'sess, ItemEnum<'ast>> {
        let name = self.parse_ident()?;
        let variants = self.parse_delim_comma_seq(Delimiter::Brace, true, Self::parse_ident)?;
        Ok(ItemEnum { name, variants })
    }

    /// Parses a user-defined value type.
    fn parse_udvt(&mut self) -> PResult<'sess, ItemUdvt<'ast>> {
        let name = self.parse_ident()?;
        self.expect_keyword(kw::Is)?;
        let ty = self.parse_type()?;
        self.expect_semi()?;
        Ok(ItemUdvt { name, ty })
    }

    /// Parses a pragma directive.
    fn parse_pragma(&mut self) -> PResult<'sess, PragmaDirective<'ast>> {
        let is_ident_or_strlit = |t: Token| t.is_ident() || t.is_str_lit();

        let tokens = if self.check_keyword(sym::solidity)
            || (self.token.is_ident()
                && self.look_ahead_with(1, |t| t.is_op() || t.is_rational_lit()))
        {
            // `pragma <ident> <req>;`
            let ident = self.parse_ident_any()?;
            let req = self.parse_semver_req()?;
            PragmaTokens::Version(ident, req)
        } else if (is_ident_or_strlit(self.token) && self.look_ahead(1).kind == TokenKind::Semi)
            || (is_ident_or_strlit(self.token)
                && self.look_ahead_with(1, is_ident_or_strlit)
                && self.look_ahead(2).kind == TokenKind::Semi)
        {
            // `pragma <k>;`
            // `pragma <k> <v>;`
            let k = self.parse_ident_or_strlit()?;
            let v = if self.token.is_ident() || self.token.is_str_lit() {
                Some(self.parse_ident_or_strlit()?)
            } else {
                None
            };
            PragmaTokens::Custom(k, v)
        } else {
            let mut tokens = Vec::new();
            while !matches!(self.token.kind, TokenKind::Semi | TokenKind::Eof) {
                tokens.push(self.token);
                self.bump();
            }
            if !self.token.is_eof() && tokens.is_empty() {
                let msg = "expected at least one token in pragma directive";
                self.dcx().err(msg).span(self.prev_token.span).emit();
            }
            PragmaTokens::Verbatim(self.alloc_vec(tokens))
        };
        self.expect_semi()?;
        Ok(PragmaDirective { tokens })
    }

    fn parse_ident_or_strlit(&mut self) -> PResult<'sess, IdentOrStrLit> {
        if self.check_ident() {
            self.parse_ident().map(IdentOrStrLit::Ident)
        } else if self.check_str_lit() {
            self.parse_str_lit().map(IdentOrStrLit::StrLit)
        } else {
            self.unexpected()
        }
    }

    /// Parses a SemVer version requirement.
    ///
    /// See `crates/ast/src/ast/semver.rs` for more details on the implementation.
    pub fn parse_semver_req(&mut self) -> PResult<'sess, SemverReq<'ast>> {
        if self.check_noexpect(TokenKind::Semi) || self.check_noexpect(TokenKind::Eof) {
            let msg = "empty version requirement";
            let span = self.prev_token.span.to(self.token.span);
            return Err(self.dcx().err(msg).span(span));
        }
        self.parse_semver_req_components_dis().map(|dis| SemverReq { dis })
    }

    /// `any(c)`
    fn parse_semver_req_components_dis(
        &mut self,
    ) -> PResult<'sess, Box<'ast, [SemverReqCon<'ast>]>> {
        // https://github.com/ethereum/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L170
        let mut dis = Vec::new();
        loop {
            dis.push(self.parse_semver_req_components_con()?);
            if self.eat(TokenKind::OrOr) {
                continue;
            }
            if self.check(TokenKind::Semi) || self.check(TokenKind::Eof) {
                break;
            }
            // `parse_semver_req_components_con` parses a single range,
            // or all the values until `||`.
            debug_assert!(
                matches!(
                    dis.last().map(|x| &x.components),
                    Some([
                        ..,
                        SemverReqComponent { span: _, kind: SemverReqComponentKind::Range(..) }
                    ])
                ),
                "not a range: last={:?}",
                dis.last()
            );
            return Err(self.dcx().err("ranges can only be combined using the || operator"));
        }
        Ok(self.alloc_vec(dis))
    }

    /// `all(c)`
    fn parse_semver_req_components_con(&mut self) -> PResult<'sess, SemverReqCon<'ast>> {
        // component - component (range)
        // or component component* (conjunction)

        let mut components = Vec::new();
        let lo = self.token.span;
        let (op, v) = self.parse_semver_component()?;
        if self.eat(TokenKind::BinOp(BinOpToken::Minus)) {
            // range
            // Ops are parsed and overwritten: https://github.com/ethereum/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L210
            let _ = op;
            let (_second_op, right) = self.parse_semver_component()?;
            let kind = SemverReqComponentKind::Range(v, right);
            let span = lo.to(self.prev_token.span);
            components.push(SemverReqComponent { span, kind });
        } else {
            // conjunction; first is already parsed
            let span = lo.to(self.prev_token.span);
            let kind = SemverReqComponentKind::Op(op, v);
            components.push(SemverReqComponent { span, kind });
            // others
            while !matches!(self.token.kind, TokenKind::OrOr | TokenKind::Eof | TokenKind::Semi) {
                let (span, (op, v)) = self.parse_spanned(Self::parse_semver_component)?;
                let kind = SemverReqComponentKind::Op(op, v);
                components.push(SemverReqComponent { span, kind });
            }
        }
        let span = lo.to(self.prev_token.span);
        let components = self.alloc_vec(components);
        Ok(SemverReqCon { span, components })
    }

    fn parse_semver_component(&mut self) -> PResult<'sess, (Option<SemverOp>, SemverVersion)> {
        let op = self.parse_semver_op();
        let v = self.parse_semver_version()?;
        Ok((op, v))
    }

    fn parse_semver_op(&mut self) -> Option<SemverOp> {
        // https://github.com/ethereum/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L227
        let op = match self.token.kind {
            TokenKind::Eq => SemverOp::Exact,
            TokenKind::Gt => SemverOp::Greater,
            TokenKind::Ge => SemverOp::GreaterEq,
            TokenKind::Lt => SemverOp::Less,
            TokenKind::Le => SemverOp::LessEq,
            TokenKind::Tilde => SemverOp::Tilde,
            TokenKind::BinOp(BinOpToken::Caret) => SemverOp::Caret,
            _ => return None,
        };
        self.bump();
        Some(op)
    }

    fn parse_semver_version(&mut self) -> PResult<'sess, SemverVersion> {
        Ok(SemverVersionParser::new(self).parse())
    }

    /// Parses an import directive.
    fn parse_import(&mut self) -> PResult<'sess, ImportDirective<'ast>> {
        let path;
        let items = if self.eat(TokenKind::BinOp(BinOpToken::Star)) {
            // * as alias from ""
            let alias = self.parse_as_alias()?;
            self.expect_keyword(sym::from)?;
            path = self.parse_str_lit()?;
            ImportItems::Glob(alias)
        } else if self.check(TokenKind::OpenDelim(Delimiter::Brace)) {
            // { x as y, ... } from ""
            let list = self.parse_delim_comma_seq(Delimiter::Brace, false, |this| {
                let name = this.parse_ident()?;
                let alias = this.parse_as_alias_opt()?;
                Ok((name, alias))
            })?;
            self.expect_keyword(sym::from)?;
            path = self.parse_str_lit()?;
            ImportItems::Aliases(list)
        } else {
            // "" as alias
            path = self.parse_str_lit()?;
            let alias = self.parse_as_alias_opt()?;
            ImportItems::Plain(alias)
        };
        if path.value.as_str().is_empty() {
            let msg = "import path cannot be empty";
            self.dcx().err(msg).span(path.span).emit();
        }
        self.expect_semi()?;
        Ok(ImportDirective { path, items })
    }

    /// Parses an optional `as` alias identifier.
    fn parse_as_alias_opt(&mut self) -> PResult<'sess, Option<Ident>> {
        if self.eat_keyword(kw::As) { self.parse_ident().map(Some) } else { Ok(None) }
    }

    /// Parses an `as` alias identifier.
    fn parse_as_alias(&mut self) -> PResult<'sess, Ident> {
        self.expect_keyword(kw::As)?;
        self.parse_ident()
    }

    /// Parses a using directive.
    fn parse_using(&mut self) -> PResult<'sess, UsingDirective<'ast>> {
        let list = self.parse_using_list()?;
        self.expect_keyword(kw::For)?;
        let ty = if self.eat(TokenKind::BinOp(BinOpToken::Star)) {
            None
        } else {
            Some(self.parse_type()?)
        };
        let global = self.eat_keyword(sym::global);
        self.expect_semi()?;
        Ok(UsingDirective { list, ty, global })
    }

    fn parse_using_list(&mut self) -> PResult<'sess, UsingList<'ast>> {
        if self.check(TokenKind::OpenDelim(Delimiter::Brace)) {
            self.parse_delim_comma_seq(Delimiter::Brace, false, |this| {
                let path = this.parse_path()?;
                let op = if this.eat_keyword(kw::As) {
                    Some(this.parse_user_definable_operator()?)
                } else {
                    None
                };
                Ok((path, op))
            })
            .map(UsingList::Multiple)
        } else {
            self.parse_path().map(UsingList::Single)
        }
    }

    fn parse_user_definable_operator(&mut self) -> PResult<'sess, UserDefinableOperator> {
        use BinOpToken::*;
        use TokenKind::*;
        use UserDefinableOperator as Op;
        macro_rules! user_op {
            ($($tok1:tt $(($tok2:tt))? => $op:expr),* $(,)?) => {
                match self.token.kind {
                    $($tok1 $(($tok2))? => $op,)*
                    _ => {
                        self.expected_tokens.extend_from_slice(&[$(ExpectedToken::Token($tok1 $(($tok2))?)),*]);
                        return self.unexpected();
                    }
                }
            };
        }
        let op = user_op! {
            BinOp(And) => Op::BitAnd,
            Tilde => Op::BitNot,
            BinOp(Or) => Op::BitOr,
            BinOp(Caret) => Op::BitXor,
            BinOp(Plus) => Op::Add,
            BinOp(Slash) => Op::Div,
            BinOp(Percent) => Op::Rem,
            BinOp(Star) => Op::Mul,
            BinOp(Minus) => Op::Sub,
            EqEq => Op::Eq,
            Ge => Op::Ge,
            Gt => Op::Gt,
            Le => Op::Le,
            Lt => Op::Lt,
            Ne => Op::Ne,
        };
        self.bump();
        Ok(op)
    }

    /* ----------------------------------------- Common ----------------------------------------- */

    /// Parses a variable declaration/definition.
    ///
    /// `state-variable-declaration`, `constant-variable-declaration`, `variable-declaration`,
    /// `variable-declaration-statement`, and more.
    pub(super) fn parse_variable_definition(
        &mut self,
        flags: VarFlags,
    ) -> PResult<'sess, VariableDefinition<'ast>> {
        self.parse_variable_definition_with(flags, None)
    }

    pub(super) fn parse_variable_definition_with(
        &mut self,
        flags: VarFlags,
        ty: Option<Type<'ast>>,
    ) -> PResult<'sess, VariableDefinition<'ast>> {
        let mut lo = self.token.span;
        let ty = match ty {
            Some(ty) => {
                lo = lo.with_lo(ty.span.lo());
                ty
            }
            None => self.parse_type()?,
        };

        if ty.is_function()
            && flags == VarFlags::STATE_VAR
            && self.check_noexpect(TokenKind::OpenDelim(Delimiter::Brace))
        {
            let msg = "expected a state variable declaration";
            let note = "this style of fallback function has been removed; use the `fallback` or `receive` keywords instead";
            self.dcx().err(msg).span(self.token.span).note(note).emit();
            let _ = self.parse_block()?;
            return Ok(VariableDefinition {
                span: lo.to(self.prev_token.span),
                ty,
                visibility: None,
                mutability: None,
                data_location: None,
                override_: None,
                indexed: false,
                name: None,
                initializer: None,
            });
        }

        let mut data_location = None;
        let mut visibility = None;
        let mut mutability = None;
        let mut override_ = None;
        let mut indexed = false;
        loop {
            if let Some(s) = self.parse_data_location() {
                if !flags.contains(VarFlags::DATALOC) {
                    let msg = "data locations are not allowed here";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else if data_location.is_some() {
                    let msg = "data location already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    data_location = Some(s);
                }
            } else if let Some(v) = self.parse_visibility() {
                if !flags.contains(VarFlags::from_visibility(v)) {
                    let msg = visibility_error(v, flags.visibilities());
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else if visibility.is_some() {
                    let msg = "visibility already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    visibility = Some(v);
                }
            } else if let Some(m) = self.parse_variable_mutability() {
                // `CONSTANT_VAR` is special cased later.
                if flags != VarFlags::CONSTANT_VAR && !flags.contains(VarFlags::from_varmut(m)) {
                    let msg = varmut_error(m, flags.varmuts());
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else if mutability.is_some() {
                    let msg = "mutability already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    mutability = Some(m);
                }
            } else if self.eat_keyword(kw::Indexed) {
                if !flags.contains(VarFlags::INDEXED) {
                    let msg = "`indexed` is not allowed here";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else if indexed {
                    let msg = "`indexed` already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    indexed = true;
                }
            } else if self.eat_keyword(kw::Virtual) {
                let msg = "`virtual` is not allowed here";
                self.dcx().err(msg).span(self.prev_token.span).emit();
            } else if self.eat_keyword(kw::Override) {
                let o = self.parse_override()?;
                if !flags.contains(VarFlags::OVERRIDE) {
                    let msg = "`override` is not allowed here";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else if override_.is_some() {
                    let msg = "override already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    override_ = Some(o);
                }
            } else {
                break;
            }
        }

        let name = if flags.contains(VarFlags::NAME) {
            self.parse_ident().map(Some)
        } else {
            self.parse_ident_opt()
        }?;
        if let Some(name) = &name
            && flags.contains(VarFlags::NAME_WARN)
        {
            debug_assert!(!flags.contains(VarFlags::NAME));
            let msg = "named function type parameters are deprecated";
            self.dcx().warn(msg).code(error_code!(6162)).span(name.span).emit();
        }

        let initializer = if flags.contains(VarFlags::INITIALIZER) && self.eat(TokenKind::Eq) {
            Some(self.parse_expr()?)
        } else {
            None
        };

        if flags.contains(VarFlags::SEMI) {
            self.expect_semi()?;
        }

        let span = lo.to(self.prev_token.span);

        if mutability == Some(VarMut::Constant) && initializer.is_none() {
            let msg = "constant variable must be initialized";
            self.dcx().err(msg).span(span).emit();
        }
        if flags == VarFlags::CONSTANT_VAR && mutability != Some(VarMut::Constant) {
            let msg = "only constant variables are allowed at file level";
            self.dcx().err(msg).span(span).emit();
        }

        Ok(VariableDefinition {
            span,
            ty,
            data_location,
            visibility,
            mutability,
            override_,
            indexed,
            name,
            initializer,
        })
    }

    /// Parses mutability of a variable: `constant | immutable`.
    fn parse_variable_mutability(&mut self) -> Option<VarMut> {
        if self.eat_keyword(kw::Constant) {
            Some(VarMut::Constant)
        } else if self.eat_keyword(kw::Immutable) {
            Some(VarMut::Immutable)
        } else {
            None
        }
    }

    /// Parses a parameter list: `($(vardecl),*)`.
    pub(super) fn parse_parameter_list(
        &mut self,
        allow_empty: bool,
        flags: VarFlags,
    ) -> PResult<'sess, ParameterList<'ast>> {
        let lo = self.token.span;
        let vars =
            self.parse_paren_comma_seq(allow_empty, |this| this.parse_variable_definition(flags))?;
        Ok(ParameterList { vars, span: lo.to(self.prev_token.span) })
    }

    /// Parses a list of inheritance specifiers.
    fn parse_inheritance(&mut self) -> PResult<'sess, Box<'ast, [Modifier<'ast>]>> {
        let mut list = SmallVec::<[_; 8]>::new();
        loop {
            list.push(self.parse_modifier()?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        Ok(self.alloc_smallvec(list))
    }

    /// Parses a storage layout specifier.
    fn parse_storage_layout_specifier(&mut self) -> PResult<'sess, StorageLayoutSpecifier<'ast>> {
        let lo = self.token.span;
        self.expect_keyword(sym::layout)?;
        self.expect_keyword(sym::at)?;
        let slot = self.parse_expr()?;
        Ok(StorageLayoutSpecifier { span: lo.to(self.prev_token.span), slot })
    }

    /// Parses a single modifier invocation.
    fn parse_modifier(&mut self) -> PResult<'sess, Modifier<'ast>> {
        let name = self.parse_path()?;
        let arguments = if self.token.kind == TokenKind::OpenDelim(Delimiter::Parenthesis) {
            self.parse_call_args()?
        } else {
            CallArgs::empty(name.span().shrink_to_hi())
        };
        Ok(Modifier { name, arguments })
    }

    /// Parses a single function override.
    ///
    /// Expects the `override` to have already been eaten.
    fn parse_override(&mut self) -> PResult<'sess, Override<'ast>> {
        debug_assert!(self.prev_token.is_keyword(kw::Override));
        let lo = self.prev_token.span;
        let paths = if self.token.is_open_delim(Delimiter::Parenthesis) {
            self.parse_paren_comma_seq(false, Self::parse_path)?
        } else {
            Default::default()
        };
        let span = lo.to(self.prev_token.span);
        Ok(Override { span, paths })
    }

    /// Parses a single string literal. This is only used in import paths and statements, not
    /// expressions.
    pub(super) fn parse_str_lit(&mut self) -> PResult<'sess, StrLit> {
        match self.parse_str_lit_opt() {
            Some(lit) => Ok(lit),
            None => self.unexpected(),
        }
    }

    /// Parses a single optional string literal. This is only used in import paths and statements,
    /// not expressions.
    pub(super) fn parse_str_lit_opt(&mut self) -> Option<StrLit> {
        if !self.check_str_lit() {
            return None;
        }
        let Token { kind: TokenKind::Literal(TokenLitKind::Str, symbol), span } = self.token else {
            unreachable!()
        };
        self.bump();
        Some(StrLit { span, value: symbol })
    }

    /// Parses a storage location: `storage | memory | calldata | transient`.
    fn parse_data_location(&mut self) -> Option<Spanned<DataLocation>> {
        let lo = self.token.span;
        let location = if self.eat_keyword(kw::Storage) {
            DataLocation::Storage
        } else if self.eat_keyword(kw::Memory) {
            DataLocation::Memory
        } else if self.eat_keyword(kw::Calldata) {
            DataLocation::Calldata
        } else if self.check_keyword(sym::transient)
            && !matches!(
                self.look_ahead(1).kind,
                TokenKind::Eq | TokenKind::Semi | TokenKind::CloseDelim(_) | TokenKind::Comma
            )
        {
            self.bump(); // `transient`
            DataLocation::Transient
        } else {
            return None;
        };

        Some(Spanned { span: lo.to(self.prev_token.span), data: location })
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

struct SemverVersionParser<'p, 'sess, 'ast> {
    p: &'p mut Parser<'sess, 'ast>,
    bumps: u32,
    pos_inside: u32,
}

impl<'p, 'sess, 'ast> SemverVersionParser<'p, 'sess, 'ast> {
    fn new(p: &'p mut Parser<'sess, 'ast>) -> Self {
        Self { p, bumps: 0, pos_inside: 0 }
    }

    fn emit_err(&self, msg: impl Into<DiagMsg>) {
        self.p.dcx().err(msg).span(self.current_span()).emit();
    }

    fn parse(mut self) -> SemverVersion {
        let lo = self.current_span();
        let major = self.parse_version_part();
        let mut minor = None;
        let mut patch = None;
        if self.eat_dot() {
            minor = Some(self.parse_version_part());
            if self.eat_dot() {
                patch = Some(self.parse_version_part());
            }
        }
        if self.pos_inside > 0 || self.bumps == 0 {
            self.emit_err("unexpected trailing characters");
            self.bump_token();
        }
        SemverVersion { span: lo.to(self.current_span()), major, minor, patch }
    }

    fn eat_dot(&mut self) -> bool {
        let r = self.current_char() == Some('.');
        if r {
            self.bump_char();
        }
        r
    }

    fn parse_version_part(&mut self) -> SemverVersionNumber {
        match self.current_char() {
            Some('*' | 'x' | 'X') => {
                self.bump_char();
                SemverVersionNumber::Wildcard
            }
            Some('0'..='9') => {
                let s = self.current_str().unwrap();
                let len = s.bytes().take_while(u8::is_ascii_digit).count();
                let result = s[..len].parse();
                self.bump_chars(len as u32);
                let Ok(n) = result else {
                    self.emit_err("version number too large");
                    return SemverVersionNumber::Wildcard;
                };
                SemverVersionNumber::Number(n)
            }
            _ => {
                self.emit_err("expected version number");
                self.bump_char();
                SemverVersionNumber::Wildcard
            }
        }
    }

    fn current_char(&self) -> Option<char> {
        self.current_str()?.chars().next()
    }

    fn current_str(&self) -> Option<&str> {
        self.current_token_str()?.get(self.pos_inside as usize..)
    }

    fn current_token_str(&self) -> Option<&str> {
        Some(match &self.current_token().kind {
            TokenKind::Dot => ".",
            TokenKind::BinOp(BinOpToken::Star) => "*",
            TokenKind::Ident(s) | TokenKind::Literal(_, s) => s.as_str(),
            _ => return None,
        })
    }

    fn current_token(&self) -> &Token {
        &self.p.token
    }

    fn current_span(&self) -> Span {
        let mut s = self.current_token().span;
        if self.pos_inside > 0 {
            s = s.with_lo(s.lo() + self.pos_inside);
        }
        s
    }

    fn bump_char(&mut self) {
        self.bump_chars(1);
    }

    fn bump_chars(&mut self, n: u32) {
        if let Some(s) = self.current_token_str() {
            if self.pos_inside + n >= s.len() as u32 {
                self.bump_token();
            } else {
                self.pos_inside += n;
            }
        }
    }

    fn bump_token(&mut self) {
        self.p.bump();
        self.bumps += 1;
        self.pos_inside = 0;
    }
}

bitflags::bitflags! {
    /// Flags for parsing variable declarations.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(super) struct VarFlags: u16 {
        // `ty` is always required. `name` is always optional, unless `NAME` is specified.

        const DATALOC     = 1 << 1;
        const INDEXED     = 1 << 2;

        const PRIVATE     = 1 << 3;
        const INTERNAL    = 1 << 4;
        const PUBLIC      = 1 << 5;
        const EXTERNAL    = 1 << 6; // Never accepted, just for error messages.
        const VISIBILITY  = Self::PRIVATE.bits()
                          | Self::INTERNAL.bits()
                          | Self::PUBLIC.bits()
                          | Self::EXTERNAL.bits();

        const CONSTANT    = 1 << 7;
        const IMMUTABLE   = 1 << 8;

        const OVERRIDE    = 1 << 9;

        const NAME        = 1 << 10;
        const NAME_WARN   = 1 << 11;

        const INITIALIZER = 1 << 12;
        const SEMI        = 1 << 13;

        const STRUCT       = Self::NAME.bits();
        const ERROR        = 0;
        const EVENT        = Self::INDEXED.bits();
        const FUNCTION     = Self::DATALOC.bits();
        const FUNCTION_TY  = Self::DATALOC.bits() | Self::NAME_WARN.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.stateVariableDeclaration
        const STATE_VAR    = Self::DATALOC.bits()
                           | Self::PRIVATE.bits()
                           | Self::INTERNAL.bits()
                           | Self::PUBLIC.bits()
                           | Self::CONSTANT.bits()
                           | Self::IMMUTABLE.bits()
                           | Self::OVERRIDE.bits()
                           | Self::NAME.bits()
                           | Self::INITIALIZER.bits()
                           | Self::SEMI.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.constantVariableDeclaration
        const CONSTANT_VAR = Self::CONSTANT.bits()
                           | Self::NAME.bits()
                           | Self::INITIALIZER.bits()
                           | Self::SEMI.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.variableDeclarationStatement
        const VAR = Self::DATALOC.bits() | Self::INITIALIZER.bits();
    }

    /// Flags for parsing function headers.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(super) struct FunctionFlags: u16 {
        /// Name is required.
        const NAME             = 1 << 0;
        /// Function type: parameter names are parsed, but issue a warning.
        const PARAM_NAME       = 1 << 1;
        /// Parens can be omitted.
        const NO_PARENS        = 1 << 2;

        // Visibility
        const PRIVATE          = 1 << 3;
        const INTERNAL         = 1 << 4;
        const PUBLIC           = 1 << 5;
        const EXTERNAL         = 1 << 6;
        const VISIBILITY       = Self::PRIVATE.bits()
                               | Self::INTERNAL.bits()
                               | Self::PUBLIC.bits()
                               | Self::EXTERNAL.bits();

        // StateMutability
        const PURE             = 1 << 7;
        const VIEW             = 1 << 8;
        const PAYABLE          = 1 << 9;
        const STATE_MUTABILITY = Self::PURE.bits()
                               | Self::VIEW.bits()
                               | Self::PAYABLE.bits();

        const MODIFIERS        = 1 << 10;
        const VIRTUAL          = 1 << 11;
        const OVERRIDE         = 1 << 12;

        const RETURNS          = 1 << 13;
        /// Must be implemented, meaning it must end in a `{}` implementation block.
        const ONLY_BLOCK       = 1 << 14;

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.constructorDefinition
        const CONSTRUCTOR = Self::MODIFIERS.bits()
                          | Self::PAYABLE.bits()
                          | Self::INTERNAL.bits()
                          | Self::PUBLIC.bits()
                          | Self::ONLY_BLOCK.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.functionDefinition
        const FUNCTION    = Self::NAME.bits()
                          | Self::VISIBILITY.bits()
                          | Self::STATE_MUTABILITY.bits()
                          | Self::MODIFIERS.bits()
                          | Self::VIRTUAL.bits()
                          | Self::OVERRIDE.bits()
                          | Self::RETURNS.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.modifierDefinition
        const MODIFIER    = Self::NAME.bits()
                          | Self::NO_PARENS.bits()
                          | Self::VIRTUAL.bits()
                          | Self::OVERRIDE.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.fallbackFunctionDefinition
        const FALLBACK    = Self::EXTERNAL.bits()
                          | Self::STATE_MUTABILITY.bits()
                          | Self::MODIFIERS.bits()
                          | Self::VIRTUAL.bits()
                          | Self::OVERRIDE.bits()
                          | Self::RETURNS.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.receiveFunctionDefinition
        const RECEIVE     = Self::EXTERNAL.bits()
                          | Self::PAYABLE.bits()
                          | Self::MODIFIERS.bits()
                          | Self::VIRTUAL.bits()
                          | Self::OVERRIDE.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.functionTypeName
        const FUNCTION_TY = Self::PARAM_NAME.bits()
                          | Self::VISIBILITY.bits()
                          | Self::STATE_MUTABILITY.bits()
                          | Self::RETURNS.bits();
    }
}

impl VarFlags {
    fn from_visibility(v: Visibility) -> Self {
        match v {
            Visibility::Private => Self::PRIVATE,
            Visibility::Internal => Self::INTERNAL,
            Visibility::Public => Self::PUBLIC,
            Visibility::External => Self::EXTERNAL,
        }
    }

    fn into_visibility(self) -> Option<Visibility> {
        match self {
            Self::PRIVATE => Some(Visibility::Private),
            Self::INTERNAL => Some(Visibility::Internal),
            Self::PUBLIC => Some(Visibility::Public),
            Self::EXTERNAL => Some(Visibility::External),
            _ => None,
        }
    }

    fn visibilities(self) -> Option<impl Iterator<Item = Visibility>> {
        self.supported(Self::VISIBILITY).map(|iter| iter.map(|x| x.into_visibility().unwrap()))
    }

    fn from_varmut(v: VarMut) -> Self {
        match v {
            VarMut::Constant => Self::CONSTANT,
            VarMut::Immutable => Self::IMMUTABLE,
        }
    }

    fn into_varmut(self) -> Option<VarMut> {
        match self {
            Self::CONSTANT => Some(VarMut::Constant),
            Self::IMMUTABLE => Some(VarMut::Immutable),
            _ => None,
        }
    }

    fn varmuts(self) -> Option<impl Iterator<Item = VarMut>> {
        self.supported(Self::CONSTANT | Self::IMMUTABLE)
            .map(|iter| iter.map(|x| x.into_varmut().unwrap()))
    }

    fn supported(self, what: Self) -> Option<impl Iterator<Item = Self>> {
        let s = self.intersection(what);
        if s.is_empty() { None } else { Some(s.iter()) }
    }
}

impl FunctionFlags {
    fn from_kind(kind: FunctionKind) -> Self {
        match kind {
            FunctionKind::Constructor => Self::CONSTRUCTOR,
            FunctionKind::Function => Self::FUNCTION,
            FunctionKind::Modifier => Self::MODIFIER,
            FunctionKind::Receive => Self::RECEIVE,
            FunctionKind::Fallback => Self::FALLBACK,
        }
    }

    fn from_visibility(visibility: Visibility) -> Self {
        match visibility {
            Visibility::Private => Self::PRIVATE,
            Visibility::Internal => Self::INTERNAL,
            Visibility::Public => Self::PUBLIC,
            Visibility::External => Self::EXTERNAL,
        }
    }

    fn into_visibility(self) -> Option<Visibility> {
        match self {
            Self::PRIVATE => Some(Visibility::Private),
            Self::INTERNAL => Some(Visibility::Internal),
            Self::PUBLIC => Some(Visibility::Public),
            Self::EXTERNAL => Some(Visibility::External),
            _ => None,
        }
    }

    fn visibilities(self) -> Option<impl Iterator<Item = Visibility>> {
        self.supported(Self::VISIBILITY).map(|iter| iter.map(|x| x.into_visibility().unwrap()))
    }

    fn from_state_mutability(state_mutability: StateMutability) -> Self {
        match state_mutability {
            StateMutability::Pure => Self::PURE,
            StateMutability::View => Self::VIEW,
            StateMutability::Payable => Self::PAYABLE,
            StateMutability::NonPayable => unreachable!("NonPayable should not be parsed"),
        }
    }

    fn into_state_mutability(self) -> Option<StateMutability> {
        match self {
            Self::PURE => Some(StateMutability::Pure),
            Self::VIEW => Some(StateMutability::View),
            Self::PAYABLE => Some(StateMutability::Payable),
            _ => None,
        }
    }

    fn state_mutabilities(self) -> Option<impl Iterator<Item = StateMutability>> {
        self.supported(Self::STATE_MUTABILITY)
            .map(|iter| iter.map(|x| x.into_state_mutability().unwrap()))
    }

    fn supported(self, what: Self) -> Option<impl Iterator<Item = Self>> {
        let s = self.intersection(what);
        if s.is_empty() { None } else { Some(s.iter()) }
    }
}

fn visibility_error(v: Visibility, iter: Option<impl Iterator<Item = Visibility>>) -> String {
    common_flags_error(v, "visibility", iter)
}

fn varmut_error(m: VarMut, iter: Option<impl Iterator<Item = VarMut>>) -> String {
    common_flags_error(m, "mutability", iter)
}

fn state_mutability_error(
    m: StateMutability,
    iter: Option<impl Iterator<Item = StateMutability>>,
) -> String {
    common_flags_error(m, "state mutability", iter)
}

fn common_flags_error<T: std::fmt::Display>(
    t: T,
    desc: &str,
    iter: Option<impl Iterator<Item = T>>,
) -> String {
    match iter {
        Some(iter) => format!("`{t}` not allowed here; allowed values: {}", iter.format(", ")),
        None => format!("{desc} is not allowed here"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::{Result, Session, source_map::FileName};

    fn assert_version_matches(tests: &[(&str, &str, bool)]) {
        let sess = Session::builder().with_test_emitter().build();
        sess.enter(|| -> Result {
            for (i, &(v, req_s, res)) in tests.iter().enumerate() {
                let name = i.to_string();
                let src = format!("{v} {req_s}");
                let arena = Arena::new();
                let mut parser =
                    Parser::from_source_code(&sess, &arena, FileName::Custom(name), src)?;

                let version = parser.parse_semver_version().map_err(|e| e.emit()).unwrap();
                assert_eq!(version.to_string(), v);
                let req: SemverReq<'_> = parser.parse_semver_req().map_err(|e| e.emit()).unwrap();
                sess.dcx.has_errors().unwrap();
                assert_eq!(req.matches(&version), res, "v={v:?}, req={req_s:?}");
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn semver_matches() {
        assert_version_matches(&[
            // none = eq
            ("0.8.1", "0", true),
            ("0.8.1", "1", false),
            ("0.8.1", "1.0", false),
            ("0.8.1", "1.0.0", false),
            ("0.8.1", "0.7", false),
            ("0.8.1", "0.7.0", false),
            ("0.8.1", "0.7.1", false),
            ("0.8.1", "0.7.2", false),
            ("0.8.1", "0.8", true),
            ("0.8.1", "0.8.0", false),
            ("0.8.1", "0.8.1", true),
            ("0.8.1", "0.8.2", false),
            ("0.8.1", "0.9", false),
            ("0.8.1", "0.9.0", false),
            ("0.8.1", "0.9.1", false),
            ("0.8.1", "0.9.2", false),
            // eq
            ("0.8.1", "=0", true),
            ("0.8.1", "=1", false),
            ("0.8.1", "=1.0", false),
            ("0.8.1", "=1.0.0", false),
            ("0.8.1", "=0.7", false),
            ("0.8.1", "=0.7.0", false),
            ("0.8.1", "=0.7.1", false),
            ("0.8.1", "=0.7.2", false),
            ("0.8.1", "=0.8", true),
            ("0.8.1", "=0.8.0", false),
            ("0.8.1", "=0.8.1", true),
            ("0.8.1", "=0.8.2", false),
            ("0.8.1", "=0.9", false),
            ("0.8.1", "=0.9.0", false),
            ("0.8.1", "=0.9.1", false),
            ("0.8.1", "=0.9.2", false),
            // gt
            ("0.8.1", ">0", false),
            ("0.8.1", ">1", false),
            ("0.8.1", ">1.0", false),
            ("0.8.1", ">1.0.0", false),
            ("0.8.1", ">0.7", true),
            ("0.8.1", ">0.7.0", true),
            ("0.8.1", ">0.7.1", true),
            ("0.8.1", ">0.7.2", true),
            ("0.8.1", ">0.8", false),
            ("0.8.1", ">0.8.0", true),
            ("0.8.1", ">0.8.1", false),
            ("0.8.1", ">0.8.2", false),
            ("0.8.1", ">0.9", false),
            ("0.8.1", ">0.9.0", false),
            ("0.8.1", ">0.9.1", false),
            ("0.8.1", ">0.9.2", false),
            // ge
            ("0.8.1", ">=0", true),
            ("0.8.1", ">=1", false),
            ("0.8.1", ">=1.0", false),
            ("0.8.1", ">=1.0.0", false),
            ("0.8.1", ">=0.7", true),
            ("0.8.1", ">=0.7.0", true),
            ("0.8.1", ">=0.7.1", true),
            ("0.8.1", ">=0.7.2", true),
            ("0.8.1", ">=0.8", true),
            ("0.8.1", ">=0.8.0", true),
            ("0.8.1", ">=0.8.1", true),
            ("0.8.1", ">=0.8.2", false),
            ("0.8.1", ">=0.9", false),
            ("0.8.1", ">=0.9.0", false),
            ("0.8.1", ">=0.9.1", false),
            ("0.8.1", ">=0.9.2", false),
            // lt
            ("0.8.1", "<0", false),
            ("0.8.1", "<1", true),
            ("0.8.1", "<1.0", true),
            ("0.8.1", "<1.0.0", true),
            ("0.8.1", "<0.7", false),
            ("0.8.1", "<0.7.0", false),
            ("0.8.1", "<0.7.1", false),
            ("0.8.1", "<0.7.2", false),
            ("0.8.1", "<0.8", false),
            ("0.8.1", "<0.8.0", false),
            ("0.8.1", "<0.8.1", false),
            ("0.8.1", "<0.8.2", true),
            ("0.8.1", "<0.9", true),
            ("0.8.1", "<0.9.0", true),
            ("0.8.1", "<0.9.1", true),
            ("0.8.1", "<0.9.2", true),
            // le
            ("0.8.1", "<=0", true),
            ("0.8.1", "<=1", true),
            ("0.8.1", "<=1.0", true),
            ("0.8.1", "<=1.0.0", true),
            ("0.8.1", "<=0.7", false),
            ("0.8.1", "<=0.7.0", false),
            ("0.8.1", "<=0.7.1", false),
            ("0.8.1", "<=0.7.2", false),
            ("0.8.1", "<=0.8", true),
            ("0.8.1", "<=0.8.0", false),
            ("0.8.1", "<=0.8.1", true),
            ("0.8.1", "<=0.8.2", true),
            ("0.8.1", "<=0.9.0", true),
            ("0.8.1", "<=0.9.1", true),
            ("0.8.1", "<=0.9.2", true),
            // tilde
            ("0.8.1", "~0", true),
            ("0.8.1", "~1", false),
            ("0.8.1", "~1.0", false),
            ("0.8.1", "~1.0.0", false),
            ("0.8.1", "~0.7", false),
            ("0.8.1", "~0.7.0", false),
            ("0.8.1", "~0.7.1", false),
            ("0.8.1", "~0.7.2", false),
            ("0.8.1", "~0.8", true),
            ("0.8.1", "~0.8.0", true),
            ("0.8.1", "~0.8.1", true),
            ("0.8.1", "~0.8.2", false),
            ("0.8.1", "~0.9.0", false),
            ("0.8.1", "~0.9.1", false),
            ("0.8.1", "~0.9.2", false),
            // caret
            ("0.8.1", "^0", true),
            ("0.8.1", "^1", false),
            ("0.8.1", "^1.0", false),
            ("0.8.1", "^1.0.0", false),
            ("0.8.1", "^0.7", false),
            ("0.8.1", "^0.7.0", false),
            ("0.8.1", "^0.7.1", false),
            ("0.8.1", "^0.7.2", false),
            ("0.8.1", "^0.8", true),
            ("0.8.1", "^0.8.0", true),
            ("0.8.1", "^0.8.1", true),
            ("0.8.1", "^0.8.2", false),
            ("0.8.1", "^0.9.0", false),
            ("0.8.1", "^0.9.1", false),
            ("0.8.1", "^0.9.2", false),
            // ranges
            ("0.8.1", "0 - 1", true),
            ("0.8.1", "0.1 - 1.1", true),
            ("0.8.1", "0.1.1 - 1.1.1", true),
            ("0.8.1", "0 - 0.8.1", true),
            ("0.8.1", "0 - 0.8.2", true),
            ("0.8.1", "0.7 - 0.8.1", true),
            ("0.8.1", "0.7 - 0.8.2", true),
            ("0.8.1", "0.8 - 0.8.1", true),
            ("0.8.1", "0.8 - 0.8.2", true),
            ("0.8.1", "0.8.0 - 0.8.1", true),
            ("0.8.1", "0.8.0 - 0.8.2", true),
            ("0.8.1", "0.8.0 - 0.9.0", true),
            ("0.8.1", "0.8.0 - 1.0.0", true),
            ("0.8.1", "0.8.1 - 0.8.1", true),
            ("0.8.1", "0.8.1 - 0.8.2", true),
            ("0.8.1", "0.8.1 - 0.9.0", true),
            ("0.8.1", "0.8.1 - 1.0.0", true),
            ("0.8.1", "0.7 - 0.8", true),
            ("0.8.1", "0.7.0 - 0.8", true),
            ("0.8.1", "0.8 - 0.8", true),
            ("0.8.1", "0.8.0 - 0.8", true),
            ("0.8.1", "0.8 - 0.8.0", false),
            ("0.8.1", "0.8 - 0.8.1", true),
            // or
            ("0.8.1", "0 || 0", true),
            ("0.8.1", "0 || 1", true),
            ("0.8.1", "1 || 0", true),
            ("0.8.1", "0.0 || 0.0", false),
            ("0.8.1", "0.0 || 1.0", false),
            ("0.8.1", "1.0 || 0.0", false),
            ("0.8.1", "0.7 || 0.8", true),
            ("0.8.1", "0.8 || 0.8", true),
            ("0.8.1", "0.8 || 0.8.1", true),
            ("0.8.1", "0.8 || 0.8.2", true),
            ("0.8.1", "0.8 || 0.9", true),
        ]);
    }

    #[test]
    /// Test if the span of a function header is correct (should start at the function-like kw and
    /// end at the last token)
    fn function_header_span() {
        let test_functions = [
            "function foo(uint256 a) public view returns (uint256) {
}",
            "modifier foo() {
    _;
}",
            "receive() external payable {
}",
            "fallback() external payable {
}",
            "constructor() {
}",
        ];

        let test_function_headers = [
            "function foo(uint256 a) public view returns (uint256)",
            "modifier foo()",
            "receive() external payable",
            "fallback() external payable",
            "constructor()",
        ];

        for (idx, src) in test_functions.iter().enumerate() {
            let sess = Session::builder().with_test_emitter().build();
            sess.enter(|| -> Result {
                let arena = Arena::new();
                let mut parser = Parser::from_source_code(
                    &sess,
                    &arena,
                    FileName::Custom(String::from("test")),
                    *src,
                )?;

                parser.in_contract = true; // Silence the wrong scope error

                let header_span = parser.parse_function().unwrap().header.span;

                assert_eq!(
                    header_span,
                    Span::new(
                        solar_interface::BytePos(0),
                        solar_interface::BytePos(test_function_headers[idx].len() as u32,),
                    ),
                );

                Ok(())
            })
            .unwrap();
        }
    }

    #[test]
    /// Test if the individual spans in function headers are correct
    fn function_header_field_spans() {
        let test_cases = vec![
            ("function foo() public {}", Some("public"), None, None, "()", None),
            ("function foo() private view {}", Some("private"), Some("view"), None, "()", None),
            (
                "function foo() internal pure returns (uint) {}",
                Some("internal"),
                Some("pure"),
                None,
                "()",
                Some("(uint)"),
            ),
            (
                "function foo() external payable {}",
                Some("external"),
                Some("payable"),
                None,
                "()",
                None,
            ),
            ("function foo() pure {}", None, Some("pure"), None, "()", None),
            ("function foo() view {}", None, Some("view"), None, "()", None),
            ("function foo() payable {}", None, Some("payable"), None, "()", None),
            ("function foo() {}", None, None, None, "()", None),
            ("function foo(uint a) {}", None, None, None, "(uint a)", None),
            ("function foo(uint a, string b) {}", None, None, None, "(uint a, string b)", None),
            ("function foo() returns (uint) {}", None, None, None, "()", Some("(uint)")),
            (
                "function foo() returns (uint, bool) {}",
                None,
                None,
                None,
                "()",
                Some("(uint, bool)"),
            ),
            (
                "function foo(uint x) public view returns (bool) {}",
                Some("public"),
                Some("view"),
                None,
                "(uint x)",
                Some("(bool)"),
            ),
            ("function foo() public virtual {}", Some("public"), None, Some("virtual"), "()", None),
            ("function foo() virtual public {}", Some("public"), None, Some("virtual"), "()", None),
            (
                "function foo() public virtual view {}",
                Some("public"),
                Some("view"),
                Some("virtual"),
                "()",
                None,
            ),
            ("function foo() virtual override {}", None, None, Some("virtual"), "()", None),
            ("modifier bar() virtual {}", None, None, Some("virtual"), "()", None),
            (
                "function foo() public virtual returns (uint) {}",
                Some("public"),
                None,
                Some("virtual"),
                "()",
                Some("(uint)"),
            ),
        ];

        let sess = Session::builder().with_test_emitter().build();
        sess.enter(|| -> Result {
            for (idx, (src, vis, sm, virt, params, returns)) in test_cases.iter().enumerate() {
                let arena = Arena::new();
                let mut parser = Parser::from_source_code(
                    &sess,
                    &arena,
                    FileName::Custom(format!("test_{idx}")),
                    *src,
                )?;
                parser.in_contract = true;

                let func = parser.parse_function().unwrap();
                let header = &func.header;

                if let Some(expected) = vis {
                    let vis_span = header.visibility.as_ref().expect("Expected visibility").span;
                    let vis_text = sess.source_map().span_to_snippet(vis_span).unwrap();
                    assert_eq!(vis_text, *expected, "Test {idx}: visibility span mismatch");
                }
                if let Some(expected) = sm
                    && let Some(state_mutability) = header.state_mutability
                {
                    assert_eq!(
                        *expected,
                        sess.source_map().span_to_snippet(state_mutability.span).unwrap(),
                        "Test {idx}: state mutability span mismatch",
                    );
                }
                if let Some(expected) = virt {
                    let virtual_span = header.virtual_.expect("Expected virtual span");
                    let virtual_text = sess.source_map().span_to_snippet(virtual_span).unwrap();
                    assert_eq!(virtual_text, *expected, "Test {idx}: virtual span mismatch");
                }
                let span = header.parameters.span;
                assert_eq!(
                    *params,
                    sess.source_map().span_to_snippet(span).unwrap(),
                    "Test {idx}: params span mismatch"
                );
                if let Some(expected) = returns {
                    let span = header.returns.as_ref().expect("Expected returns").span;
                    assert_eq!(
                        *expected,
                        sess.source_map().span_to_snippet(span).unwrap(),
                        "Test {idx}: returns span mismatch",
                    );
                }
            }
            Ok(())
        })
        .unwrap();
    }
}
