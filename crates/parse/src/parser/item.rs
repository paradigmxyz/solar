use super::{ExpectedToken, SeqSep};
use crate::{PResult, Parser};
use itertools::Itertools;
use std::num::IntErrorKind;
use sulk_ast::{ast::*, token::*};
use sulk_interface::{error_code, kw, sym, Ident, Span};

impl<'sess, 'ast> Parser<'sess, 'ast> {
    /// Parses a source unit.
    #[instrument(level = "debug", skip_all)]
    pub fn parse_file(&mut self) -> PResult<'sess, SourceUnit<'ast>> {
        self.parse_items(&TokenKind::Eof).map(SourceUnit::new)
    }

    /// Parses a list of items until the given token is encountered.
    fn parse_items(&mut self, end: &TokenKind) -> PResult<'sess, Box<'ast, [Item<'ast>]>> {
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
            let note = format!("for a full list of valid {prefix} items, see <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.{link}>");
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
        let docs = self.parse_doc_comments()?;
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
        let body = if !flags.contains(FunctionFlags::ONLY_BLOCK) && self.eat(&TokenKind::Semi) {
            None
        } else {
            Some(self.parse_block()?)
        };

        if !self.in_contract && !kind.allowed_in_global() {
            let msg = format!("{kind}s are not allowed in the global scope");
            self.dcx().err(msg).span(lo.to(self.prev_token.span)).emit();
        }
        // All function kinds are allowed in contracts.

        Ok(ItemFunction { kind, header, body })
    }

    /// Parses a function a header.
    pub(super) fn parse_function_header(
        &mut self,
        flags: FunctionFlags,
    ) -> PResult<'sess, FunctionHeader<'ast>> {
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
                    let help = format!("remove the `function` keyword if you intend this to be a contract's {ident} function");
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
            // HACK: Ugly way to add an extra guard to `if let` without the unstable `let-chains`.
            // Ideally this would be `if let Some(_) = _ && guard { ... }`.
            let vis_guard = (!(flags == FunctionFlags::FUNCTION_TY && header.visibility.is_some()))
                .then_some(());
            if let Some(visibility) = vis_guard.and_then(|()| self.parse_visibility()) {
                if !flags.contains(FunctionFlags::from_visibility(visibility)) {
                    let msg = visibility_error(visibility, flags.visibilities());
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else if header.visibility.is_some() {
                    let msg = "visibility already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    header.visibility = Some(visibility);
                }
            } else if let Some(state_mutability) = self.parse_state_mutability() {
                if !flags.contains(FunctionFlags::from_state_mutability(state_mutability)) {
                    let msg = state_mutability_error(state_mutability, flags.state_mutabilities());
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else if header.state_mutability.is_some() {
                    let msg = "state mutability already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    header.state_mutability = Some(state_mutability);
                }
            } else if self.eat_keyword(kw::Virtual) {
                if !flags.contains(FunctionFlags::VIRTUAL) {
                    let msg = "`virtual` is not allowed here";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else if header.virtual_ {
                    let msg = "virtual already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    header.virtual_ = true;
                }
            } else if self.eat_keyword(kw::Override) {
                let o = self.parse_override()?;
                if !flags.contains(FunctionFlags::OVERRIDE) {
                    let msg = "`override` is not allowed here";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else if header.override_.is_some() {
                    let msg = "override already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
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
            header.returns = self.parse_parameter_list(false, var_flags)?;
        }

        Ok(header)
    }

    /// Parses a struct definition.
    fn parse_struct(&mut self) -> PResult<'sess, ItemStruct<'ast>> {
        let name = self.parse_ident()?;
        let (fields, _) = self.parse_delim_seq(
            Delimiter::Brace,
            SeqSep::trailing_enforced(TokenKind::Semi),
            false,
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
        let bases =
            if self.eat_keyword(kw::Is) { self.parse_inheritance()? } else { Default::default() };
        self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?;
        let body =
            self.in_contract(|this| this.parse_items(&TokenKind::CloseDelim(Delimiter::Brace)))?;
        Ok(ItemContract { kind, name, bases, body })
    }

    /// Parses an enum definition.
    fn parse_enum(&mut self) -> PResult<'sess, ItemEnum<'ast>> {
        let name = self.parse_ident()?;
        let (variants, _) = self.parse_delim_comma_seq(Delimiter::Brace, false, |this| {
            // Ignore doc-comments.
            let _ = this.parse_doc_comments()?;
            this.parse_ident()
        })?;
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
        let is_ident_or_strlit = |t: &Token| t.is_ident() || t.is_str_lit();

        let tokens = if self.check_keyword(sym::solidity)
            || (self.token.is_ident()
                && self.look_ahead_with(1, |t| t.is_op() || t.is_rational_lit()))
        {
            // `pragma <ident> <req>;`
            let ident = self.parse_ident_any()?;
            let req = self.parse_semver_req()?;
            PragmaTokens::Version(ident, req)
        } else if (is_ident_or_strlit(&self.token) && self.look_ahead(1).kind == TokenKind::Semi)
            || (is_ident_or_strlit(&self.token)
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
                tokens.push(self.token.clone());
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
    fn parse_semver_req(&mut self) -> PResult<'sess, SemverReq<'ast>> {
        if self.check_noexpect(&TokenKind::Semi) {
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
            if self.eat(&TokenKind::OrOr) {
                continue;
            }
            if self.check(&TokenKind::Semi) || self.check(&TokenKind::Eof) {
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
        if self.eat(&TokenKind::BinOp(BinOpToken::Minus)) {
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
        let lo = self.token.span;
        let major;
        let mut minor = None;
        let mut patch = None;
        // Special case: `number.number` gets lexed as a rational literal.
        // In the comments `*` also represents `x` or `X`.
        if self.token.is_rational_lit() {
            // 0.1 .2
            let lit = self.token.lit().unwrap();
            let (mj, mn) = lit.symbol.as_str().split_once('.').unwrap();
            major = SemverVersionNumber::Number(self.parse_u32(mj, self.token.span));
            minor = Some(SemverVersionNumber::Number(self.parse_u32(mn, self.token.span)));
            self.bump();

            patch =
                if self.eat(&TokenKind::Dot) { Some(self.parse_semver_number()?) } else { None };
        } else {
            // (0 )|\*\.1 ?\.2
            major = self.parse_semver_number()?;
            if self.eat(&TokenKind::Dot) {
                if self.token.is_rational_lit() {
                    // *. 1.2
                    let lit = self.token.lit().unwrap();
                    let (mn, p) = lit.symbol.as_str().split_once('.').unwrap();
                    minor = Some(SemverVersionNumber::Number(self.parse_u32(mn, self.token.span)));
                    patch = Some(SemverVersionNumber::Number(self.parse_u32(p, self.token.span)));
                    self.bump();
                } else {
                    // *.1 .2
                    minor = Some(self.parse_semver_number()?);
                    patch = if self.eat(&TokenKind::Dot) {
                        Some(self.parse_semver_number()?)
                    } else {
                        None
                    };
                }
            }
        }
        let span = lo.to(self.prev_token.span);
        Ok(SemverVersion { span, major, minor, patch })
    }

    fn parse_semver_number(&mut self) -> PResult<'sess, SemverVersionNumber> {
        if self.check_noexpect(&TokenKind::BinOp(BinOpToken::Star))
            || self.token.is_keyword_any(&[sym::x, sym::X])
        {
            self.bump();
            return Ok(SemverVersionNumber::Wildcard);
        }

        let Token {
            kind: TokenKind::Literal(TokenLit { kind: TokenLitKind::Integer, symbol }),
            span,
        } = self.token
        else {
            self.expected_tokens.push(ExpectedToken::VersionNumber);
            return self.unexpected();
        };
        let value = self.parse_u32(symbol.as_str(), span);
        self.bump();
        Ok(SemverVersionNumber::Number(value))
    }

    fn parse_u32(&mut self, s: &str, span: Span) -> u32 {
        match s.parse::<u32>() {
            Ok(n) => n,
            Err(e) => match e.kind() {
                IntErrorKind::Empty => 0,
                _ => {
                    self.dcx().err(e.to_string()).span(span).emit();
                    u32::MAX
                }
            },
        }
    }

    /// Parses an import directive.
    fn parse_import(&mut self) -> PResult<'sess, ImportDirective<'ast>> {
        let path;
        let items = if self.eat(&TokenKind::BinOp(BinOpToken::Star)) {
            // * as alias from ""
            let alias = self.parse_as_alias()?;
            self.expect_keyword(sym::from)?;
            path = self.parse_str_lit()?;
            ImportItems::Glob(alias)
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            // { x as y, ... } from ""
            let (list, _) = self.parse_delim_comma_seq(Delimiter::Brace, false, |this| {
                let name = this.parse_ident()?;
                let alias = this.parse_as_alias()?;
                Ok((name, alias))
            })?;
            self.expect_keyword(sym::from)?;
            path = self.parse_str_lit()?;
            ImportItems::Aliases(list)
        } else {
            // "" as alias
            path = self.parse_str_lit()?;
            let alias = self.parse_as_alias()?;
            ImportItems::Plain(alias)
        };
        if path.value.as_str().is_empty() {
            let msg = "import path cannot be empty";
            self.dcx().err(msg).span(path.span).emit();
        }
        self.expect_semi()?;
        Ok(ImportDirective { path, items })
    }

    /// Parses an `as` alias identifier.
    fn parse_as_alias(&mut self) -> PResult<'sess, Option<Ident>> {
        if self.eat_keyword(kw::As) {
            self.parse_ident().map(Some)
        } else {
            Ok(None)
        }
    }

    /// Parses a using directive.
    fn parse_using(&mut self) -> PResult<'sess, UsingDirective<'ast>> {
        let list = self.parse_using_list()?;
        self.expect_keyword(kw::For)?;
        let ty = if self.eat(&TokenKind::BinOp(BinOpToken::Star)) {
            None
        } else {
            Some(self.parse_type()?)
        };
        let global = self.eat_keyword(sym::global);
        self.expect_semi()?;
        Ok(UsingDirective { list, ty, global })
    }

    fn parse_using_list(&mut self) -> PResult<'sess, UsingList<'ast>> {
        if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            let (paths, _) = self.parse_delim_comma_seq(Delimiter::Brace, false, |this| {
                let path = this.parse_path()?;
                let op = if this.eat_keyword(kw::As) {
                    Some(this.parse_user_definable_operator()?)
                } else {
                    None
                };
                Ok((path, op))
            })?;
            Ok(UsingList::Multiple(paths))
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
        let mut span = self.token.span;
        let ty = match ty {
            Some(ty) => {
                span = span.with_lo(ty.span.lo());
                ty
            }
            None => {
                // Ignore doc-comments.
                let _ = self.parse_doc_comments()?;
                self.parse_type()?
            }
        };

        let mut data_location = None;
        let mut visibility = None;
        let mut mutability = None;
        let mut override_ = None;
        let mut indexed = false;
        loop {
            if let Some(s) = self.parse_data_location() {
                if !flags.contains(VarFlags::DATALOC) {
                    let msg = "storage specifiers are not allowed here";
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
                if !flags.contains(VarFlags::from_varmut(m)) {
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
        if let Some(name) = &name {
            if flags.contains(VarFlags::NAME_WARN) {
                debug_assert!(!flags.contains(VarFlags::NAME));
                let msg = "named function type parameters are deprecated";
                self.dcx().warn(msg).code(error_code!(6162)).span(name.span).emit();
            }
        }

        let initializer = if flags.contains(VarFlags::INITIALIZER) && self.eat(&TokenKind::Eq) {
            Some(self.parse_expr()?)
        } else {
            None
        };

        if flags.contains(VarFlags::SEMI) {
            self.expect_semi()?;
        }

        let span = span.to(self.prev_token.span);

        if mutability == Some(VarMut::Constant) && initializer.is_none() {
            let msg = "constant variable must be initialized";
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
        self.parse_paren_comma_seq(allow_empty, |this| this.parse_variable_definition(flags))
            .map(|(x, _)| x)
    }

    /// Parses a list of inheritance specifiers.
    fn parse_inheritance(&mut self) -> PResult<'sess, Box<'ast, [Modifier<'ast>]>> {
        self.parse_seq_to_before_end(
            &TokenKind::OpenDelim(Delimiter::Brace),
            SeqSep::trailing_disallowed(TokenKind::Comma),
            false,
            Self::parse_modifier,
        )
        .map(|(x, _, _)| x)
    }

    /// Parses a single modifier invocation.
    fn parse_modifier(&mut self) -> PResult<'sess, Modifier<'ast>> {
        let name = self.parse_path()?;
        let arguments = if self.token.kind == TokenKind::OpenDelim(Delimiter::Parenthesis) {
            self.parse_call_args()?
        } else {
            CallArgs::empty()
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
            self.parse_paren_comma_seq(false, Self::parse_path)?.0
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
        let Token { kind: TokenKind::Literal(TokenLit { kind: TokenLitKind::Str, symbol }), span } =
            self.token
        else {
            unreachable!()
        };
        self.bump();
        Some(StrLit { span, value: symbol })
    }

    /// Parses a storage location: `storage | memory | calldata`.
    pub(super) fn parse_data_location(&mut self) -> Option<DataLocation> {
        if self.eat_keyword(kw::Storage) {
            Some(DataLocation::Storage)
        } else if self.eat_keyword(kw::Memory) {
            Some(DataLocation::Memory)
        } else if self.eat_keyword(kw::Calldata) {
            Some(DataLocation::Calldata)
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

bitflags::bitflags! {
    /// Flags for parsing variable declarations.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(super) struct VarFlags: u16 {
        // `ty` is always required. `name` is always optional, unless `NAME` is specified.
        const DATALOC     = 1 << 0;
        const INDEXED     = 1 << 1;

        const PRIVATE     = 1 << 2;
        const INTERNAL    = 1 << 3;
        const PUBLIC      = 1 << 4;
        const EXTERNAL    = 1 << 5; // Never accepted, just for error messages.
        const VISIBILITY  = Self::PRIVATE.bits() |
                            Self::INTERNAL.bits() |
                            Self::PUBLIC.bits() |
                            Self::EXTERNAL.bits();

        const CONSTANT    = 1 << 6;
        const IMMUTABLE   = 1 << 7;

        const OVERRIDE    = 1 << 8;

        const NAME        = 1 << 9;
        const NAME_WARN   = 1 << 10;

        const INITIALIZER = 1 << 11;
        const SEMI        = 1 << 12;

        const STRUCT       = Self::NAME.bits();
        const ERROR        = 0;
        const EVENT        = Self::INDEXED.bits();
        const FUNCTION     = Self::DATALOC.bits();
        const FUNCTION_TY  = Self::DATALOC.bits() | Self::NAME_WARN.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.stateVariableDeclaration
        const STATE_VAR    = Self::PRIVATE.bits() |
                             Self::INTERNAL.bits() |
                             Self::PUBLIC.bits() |
                             Self::CONSTANT.bits() |
                             Self::IMMUTABLE.bits() |
                             Self::OVERRIDE.bits() |
                             Self::NAME.bits() |
                             Self::INITIALIZER.bits() |
                             Self::SEMI.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.constantVariableDeclaration
        const CONSTANT_VAR = Self::CONSTANT.bits() |
                             Self::NAME.bits() |
                             Self::INITIALIZER.bits() |
                             Self::SEMI.bits();

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
        const VISIBILITY       = Self::PRIVATE.bits() |
                                 Self::INTERNAL.bits() |
                                 Self::PUBLIC.bits() |
                                 Self::EXTERNAL.bits();

        // StateMutability
        const PURE             = 1 << 7;
        const VIEW             = 1 << 8;
        const PAYABLE          = 1 << 9;
        const STATE_MUTABILITY = Self::PURE.bits() |
                                 Self::VIEW.bits() |
                                 Self::PAYABLE.bits();

        const MODIFIERS        = 1 << 10;
        const VIRTUAL          = 1 << 11;
        const OVERRIDE         = 1 << 12;

        const RETURNS          = 1 << 13;
        /// Must be implemented, meaning it must end in a `{}` implementation block.
        const ONLY_BLOCK       = 1 << 14;

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.constructorDefinition
        const CONSTRUCTOR = Self::MODIFIERS.bits() |
                            Self::PAYABLE.bits() |
                            Self::INTERNAL.bits() |
                            Self::PUBLIC.bits() |
                            Self::ONLY_BLOCK.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.functionDefinition
        const FUNCTION    = Self::NAME.bits() |
                            Self::VISIBILITY.bits() |
                            Self::STATE_MUTABILITY.bits() |
                            Self::MODIFIERS.bits() |
                            Self::VIRTUAL.bits() |
                            Self::OVERRIDE.bits() |
                            Self::RETURNS.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.modifierDefinition
        const MODIFIER    = Self::NAME.bits() |
                            Self::NO_PARENS.bits() |
                            Self::VIRTUAL.bits() |
                            Self::OVERRIDE.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.fallbackFunctionDefinition
        const FALLBACK    = Self::EXTERNAL.bits() |
                            Self::STATE_MUTABILITY.bits() |
                            Self::MODIFIERS.bits() |
                            Self::VIRTUAL.bits() |
                            Self::OVERRIDE.bits() |
                            Self::RETURNS.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.receiveFunctionDefinition
        const RECEIVE     = Self::EXTERNAL.bits() |
                            Self::PAYABLE.bits() |
                            Self::MODIFIERS.bits() |
                            Self::VIRTUAL.bits() |
                            Self::OVERRIDE.bits();

        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.functionTypeName
        const FUNCTION_TY = Self::PARAM_NAME.bits() |
                            Self::VISIBILITY.bits() |
                            Self::STATE_MUTABILITY.bits() |
                            Self::RETURNS.bits();
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
        if s.is_empty() {
            None
        } else {
            Some(s.iter())
        }
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
        if s.is_empty() {
            None
        } else {
            Some(s.iter())
        }
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
    use sulk_interface::{source_map::FileName, Result, Session};

    fn assert_version_matches(tests: &[(&str, &str, bool)]) {
        sulk_interface::enter(|| -> Result {
            let sess = Session::with_test_emitter();
            for (i, &(v, req_s, res)) in tests.iter().enumerate() {
                let name = i.to_string();
                let src = format!("{v} {req_s}");
                let arena = Arena::new();
                let mut parser =
                    Parser::from_source_code(&sess, &arena, FileName::Custom(name), src)?;

                let version = parser.parse_semver_version().map_err(|e| e.emit()).unwrap();
                assert_eq!(version.to_string(), v);
                let req = parser.parse_semver_req().map_err(|e| e.emit()).unwrap();
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
}
