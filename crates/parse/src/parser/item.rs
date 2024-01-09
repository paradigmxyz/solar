use super::{ExpectedToken, SeqSep};
use crate::{PResult, Parser};
use itertools::Itertools;
use sulk_ast::{
    ast::*,
    token::{TokenLit, TokenLitKind, *},
};
use sulk_interface::{error_code, kw, sym, Ident};

impl<'a> Parser<'a> {
    /// Parses a source unit.
    pub fn parse_file(&mut self) -> PResult<'a, SourceUnit> {
        let items = self.parse_items(&TokenKind::Eof)?;
        Ok(SourceUnit { items })
    }

    /// Parses a list of items until the given token is encountered.
    fn parse_items(&mut self, end: &TokenKind) -> PResult<'a, Vec<Item>> {
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
            }
            items.push(item);
        }
        if !self.eat(end) {
            let (msg, note) = get_msg_note(self);
            return Err(self.dcx().err(msg).span(self.token.span).note(note));
        }
        Ok(items)
    }

    /// Parses an item.
    pub fn parse_item(&mut self) -> PResult<'a, Option<Item>> {
        let docs = self.parse_doc_comments()?;
        self.parse_spanned(Self::parse_item_kind)
            .map(|(span, kind)| kind.map(|kind| Item { docs, span, kind }))
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
        } else if self.check_keyword(sym::error)
            && self.look_ahead(1).is_ident()
            && self.look_ahead(2).is_open_delim(Delimiter::Parenthesis)
        {
            self.bump(); // `error`
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
    fn parse_function(&mut self) -> PResult<'a, ItemFunction> {
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
    ) -> PResult<'a, FunctionHeader> {
        let mut header = FunctionHeader::default();
        let var_flags = if flags.contains(FunctionFlags::PARAM_NAME) {
            VarFlags::FUNCTION_TY
        } else {
            VarFlags::FUNCTION
        };

        if flags.contains(FunctionFlags::NAME) {
            header.name = Some(self.parse_ident()?);
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
            header.parameters = self.parse_parameter_list(var_flags)?;
        }

        loop {
            if let Some(visibility) = self.parse_visibility() {
                if !flags.contains(FunctionFlags::from_visibility(visibility)) {
                    let msg = match flags.visibilities() {
                        Some(iter) => format!(
                            "`{visibility}` not allowed here; allowed values: {}",
                            iter.map(|v| v.to_str()).format(", ")
                        ),
                        None => "visibility is not allowed here".into(),
                    };
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else if header.visibility.is_some() {
                    let msg = "visibility already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    header.visibility = Some(visibility);
                }
            } else if let Some(state_mutability) = self.parse_state_mutability() {
                if !flags.contains(FunctionFlags::from_state_mutability(state_mutability)) {
                    let msg = match flags.state_mutabilities() {
                        Some(iter) => format!(
                            "`{state_mutability}` not allowed here; allowed values: {}",
                            iter.map(|v| v.to_str()).format(", ")
                        ),
                        None => "state mutability is not allowed here".into(),
                    };
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
                if !flags.contains(FunctionFlags::OVERRIDE) {
                    let msg = "`override` is not allowed here";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else if header.override_.is_some() {
                    let msg = "override already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    header.override_ = Some(self.parse_override()?);
                }
            } else if flags.contains(FunctionFlags::MODIFIERS)
                && self.token.is_non_reserved_ident(false)
            {
                header.modifiers.push(self.parse_modifier()?);
            } else {
                break;
            }
        }

        if flags.contains(FunctionFlags::RETURNS) && self.eat_keyword(kw::Returns) {
            header.returns = self.parse_returns(var_flags)?;
        }

        Ok(header)
    }

    /// Parses a struct definition.
    fn parse_struct(&mut self) -> PResult<'a, ItemStruct> {
        let name = self.parse_ident()?;
        let (fields, _) = self.parse_delim_seq(
            Delimiter::Brace,
            SeqSep::trailing_enforced(TokenKind::Semi),
            false,
            |this| this.parse_variable_declaration(VarFlags::STRUCT),
        )?;
        Ok(ItemStruct { name, fields })
    }

    /// Parses an event definition.
    fn parse_event(&mut self) -> PResult<'a, ItemEvent> {
        let name = self.parse_ident()?;
        let parameters = self.parse_parameter_list(VarFlags::EVENT)?;
        let anonymous = self.eat_keyword(kw::Anonymous);
        self.expect_semi()?;
        Ok(ItemEvent { name, parameters, anonymous })
    }

    /// Parses an error definition.
    fn parse_error(&mut self) -> PResult<'a, ItemError> {
        let name = self.parse_ident()?;
        let parameters = self.parse_parameter_list(VarFlags::ERROR)?;
        self.expect_semi()?;
        Ok(ItemError { name, parameters })
    }

    /// Parses a contract definition.
    ///
    /// Expects the current token to be a contract-like keyword.
    fn parse_contract(&mut self) -> PResult<'a, ItemContract> {
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
        let inheritance =
            if self.eat_keyword(kw::Is) { self.parse_inheritance()? } else { Vec::new() };
        self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?;
        let body =
            self.in_contract(|this| this.parse_items(&TokenKind::CloseDelim(Delimiter::Brace)))?;
        Ok(ItemContract { kind, name, inheritance, body })
    }

    /// Parses an enum definition.
    fn parse_enum(&mut self) -> PResult<'a, ItemEnum> {
        let name = self.parse_ident()?;
        let (variants, _) =
            self.parse_delim_comma_seq(Delimiter::Brace, false, Self::parse_ident)?;
        Ok(ItemEnum { name, variants })
    }

    /// Parses a user-defined value type.
    fn parse_udvt(&mut self) -> PResult<'a, ItemUdvt> {
        let name = self.parse_ident()?;
        self.expect_keyword(kw::Is)?;
        let ty = self.parse_type()?;
        self.expect_semi()?;
        Ok(ItemUdvt { name, ty })
    }

    /// Parses a pragma directive.
    fn parse_pragma(&mut self) -> PResult<'a, PragmaDirective> {
        let is_ident_or_strlit = |t: &Token| t.is_ident() || t.is_str_lit();

        let tokens = if (is_ident_or_strlit(&self.token)
            && self.look_ahead(1).kind == TokenKind::Semi)
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
        } else if self.eat_keyword(sym::solidity)
            || (self.token.is_ident()
                && self.look_ahead_with(1, |t| t.is_op() || t.is_rational_lit()))
        {
            // `pragma <ident> <req>;`
            let ident = self.parse_ident_any()?;
            let req = self.parse_semver_req()?;
            PragmaTokens::Version(ident, req)
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
            PragmaTokens::Verbatim(tokens)
        };
        self.expect_semi()?;
        Ok(PragmaDirective { tokens })
    }

    fn parse_ident_or_strlit(&mut self) -> PResult<'a, IdentOrStrLit> {
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
    fn parse_semver_req(&mut self) -> PResult<'a, SemverReq> {
        if self.check_noexpect(&TokenKind::Semi) {
            let msg = "empty version requirement";
            let span = self.prev_token.span.to(self.token.span);
            return Err(self.dcx().err(msg).span(span));
        }
        self.parse_semver_req_components_dis().map(|dis| SemverReq { dis })
    }

    /// `any(c)`
    fn parse_semver_req_components_dis(&mut self) -> PResult<'a, Vec<SemverReqCon>> {
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
                    dis.last().map(|x| x.components.as_slice()),
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
        Ok(dis)
    }

    /// `all(c)`
    fn parse_semver_req_components_con(&mut self) -> PResult<'a, SemverReqCon> {
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
            // conjuction; first is already parsed
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
        Ok(SemverReqCon { span, components })
    }

    fn parse_semver_component(&mut self) -> PResult<'a, (Option<SemverOp>, SemverVersion)> {
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

    fn parse_semver_version(&mut self) -> PResult<'a, SemverVersion> {
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
            major = SemverVersionNumber::Number(self.parse_u32(mj)?);
            minor = Some(SemverVersionNumber::Number(self.parse_u32(mn)?));
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
                    minor = Some(SemverVersionNumber::Number(self.parse_u32(mn)?));
                    patch = Some(SemverVersionNumber::Number(self.parse_u32(p)?));
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

    fn parse_semver_number(&mut self) -> PResult<'a, SemverVersionNumber> {
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
        let value = self.parse_u32(symbol.as_str()).map_err(|e| e.span(span))?;
        self.bump();
        Ok(SemverVersionNumber::Number(value))
    }

    fn parse_u32(&mut self, s: &str) -> PResult<'a, u32> {
        s.parse::<u32>().map_err(|e| self.dcx().err(e.to_string()))
    }

    /// Parses an import directive.
    fn parse_import(&mut self) -> PResult<'a, ImportDirective> {
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
    fn parse_as_alias(&mut self) -> PResult<'a, Option<Ident>> {
        if self.eat_keyword(kw::As) {
            self.parse_ident().map(Some)
        } else {
            Ok(None)
        }
    }

    /// Parses a using directive.
    fn parse_using(&mut self) -> PResult<'a, UsingDirective> {
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

    fn parse_using_list(&mut self) -> PResult<'a, UsingList> {
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

    fn parse_user_definable_operator(&mut self) -> PResult<'a, UserDefinableOperator> {
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

    /// Parses a variable definition.
    fn parse_variable_definition(&mut self) -> PResult<'a, VariableDefinition> {
        let ty = self.parse_type()?;
        let mut storage = None;
        let mut visibility = None;
        let mut mutability = None;
        let mut override_ = None;
        loop {
            if let Some(s) = self.parse_storage() {
                if storage.is_some() {
                    let msg = "storage already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    storage = Some(s);
                }
            } else if let Some(v) = self.parse_visibility() {
                if visibility.is_some() {
                    let msg = "visibility already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    visibility = Some(v);
                }
            } else if let Some(m) = self.parse_variable_mutability() {
                if mutability.is_some() {
                    let msg = "mutability already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    mutability = Some(m);
                }
            } else if self.eat_keyword(kw::Override) {
                if override_.is_some() {
                    let msg = "override already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    override_ = Some(self.parse_override()?);
                }
            } else {
                break;
            }
        }
        let name = self.parse_ident()?;
        let initializer = if self.eat(&TokenKind::Eq) { Some(self.parse_expr()?) } else { None };
        self.expect_semi()?;
        Ok(VariableDefinition { ty, storage, visibility, mutability, override_, name, initializer })
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
    pub(super) fn parse_parameter_list(&mut self, flags: VarFlags) -> PResult<'a, ParameterList> {
        self.parse_paren_comma_seq(true, |this| this.parse_variable_declaration(flags))
            .map(|(x, _)| x)
    }

    /// Parses a returns list (non-emtpy parameter list).
    pub(super) fn parse_returns(&mut self, flags: VarFlags) -> PResult<'a, ParameterList> {
        self.parse_paren_comma_seq(false, |this| this.parse_variable_declaration(flags))
            .map(|(x, _)| x)
    }

    /// Parses a variable declaration: `type storage? indexed? name`.
    pub(super) fn parse_variable_declaration(
        &mut self,
        flags: VarFlags,
    ) -> PResult<'a, VariableDeclaration> {
        let ty = self.parse_type()?;

        let mut storage = None;
        let mut indexed = false;
        loop {
            if let Some(s) = self.parse_storage() {
                if !flags.contains(VarFlags::STORAGE) {
                    let msg = "storage specifiers are not allowed here";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else if storage.is_some() {
                    let msg = "storage already specified";
                    self.dcx().err(msg).span(self.prev_token.span).emit();
                } else {
                    storage = Some(s);
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
            } else {
                break;
            }
        }

        let name = if flags.contains(VarFlags::NAME) {
            self.parse_ident().map(Some)
        } else {
            self.parse_ident_opt()
        }?;
        if flags.contains(VarFlags::NAME_WARN) && name.is_some() {
            debug_assert!(!flags.contains(VarFlags::NAME));
            let msg = "named function type parameters are deprecated";
            self.dcx().warn(msg).code(error_code!(E6162)).span(self.prev_token.span).emit();
        }

        Ok(VariableDeclaration { ty, storage, indexed, name })
    }

    /// Parses a list of inheritance specifiers.
    fn parse_inheritance(&mut self) -> PResult<'a, Vec<Modifier>> {
        self.parse_seq_to_before_end(
            &TokenKind::OpenDelim(Delimiter::Brace),
            SeqSep::trailing_disallowed(TokenKind::Comma),
            false,
            Self::parse_modifier,
        )
        .map(|(x, _, _)| x)
    }

    /// Parses a single modifier invocation.
    fn parse_modifier(&mut self) -> PResult<'a, Modifier> {
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
    fn parse_override(&mut self) -> PResult<'a, Override> {
        debug_assert!(self.prev_token.is_keyword(kw::Override));
        let lo = self.prev_token.span;
        let paths = if self.token.is_open_delim(Delimiter::Parenthesis) {
            self.parse_paren_comma_seq(false, Self::parse_path)?.0
        } else {
            Vec::new()
        };
        let span = lo.to(self.prev_token.span);
        Ok(Override { span, paths })
    }

    /// Parses a single string literal. This is only used in import paths and statements, not
    /// expressions.
    pub(super) fn parse_str_lit(&mut self) -> PResult<'a, StrLit> {
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

bitflags::bitflags! {
    /// Flags for parsing variable declarations.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(crate) struct VarFlags: u8 {
        // `ty` is always required. `name` is always optional, unless `NAME` is specified.
        const STORAGE   = 1 << 1;
        const INDEXED   = 1 << 2;
        const NAME      = 1 << 3;
        const NAME_WARN = 1 << 4;

        const STRUCT      = Self::NAME.bits();
        const ERROR       = 0;
        const EVENT       = Self::INDEXED.bits();
        const FUNCTION    = Self::STORAGE.bits();
        const FUNCTION_TY = Self::STORAGE.bits() | Self::NAME_WARN.bits();
    }

    /// Flags for parsing function headers.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(super) struct FunctionFlags: u16 {
        /// Name is required.
        const NAME             = 1 << 1;
        /// Function type: parameter names are parsed, but issue a warning.
        const PARAM_NAME       = 1 << 2;
        /// Parens can be omitted.
        const NO_PARENS        = 1 << 3;

        // Visibility
        const PRIVATE          = 1 << 4;
        const INTERNAL         = 1 << 5;
        const PUBLIC           = 1 << 6;
        const EXTERNAL         = 1 << 7;
        const VISIBILITY       = Self::PRIVATE.bits() |
                                 Self::INTERNAL.bits() |
                                 Self::PUBLIC.bits() |
                                 Self::EXTERNAL.bits();

        // StateMutability
        const PURE             = 1 << 8;
        const VIEW             = 1 << 9;
        const PAYABLE          = 1 << 10;
        const STATE_MUTABILITY = Self::PURE.bits() |
                                 Self::VIEW.bits() |
                                 Self::PAYABLE.bits();

        const MODIFIERS        = 1 << 11;
        const VIRTUAL          = 1 << 12;
        const OVERRIDE         = 1 << 13;

        const RETURNS          = 1 << 14;
        /// Must be implemented, meaning it must end in a `{}` implementation block.
        const ONLY_BLOCK       = 1 << 15;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ParseSess;
    use sulk_interface::source_map::FileName;

    fn assert_version_matches(tests: &[(&str, &str, bool)]) {
        sulk_interface::enter(|| {
            let sess = ParseSess::with_test_emitter(false);
            for (i, &(v, req_s, res)) in tests.iter().enumerate() {
                let name = i.to_string();
                let src = format!("{v} {req_s}");
                let mut parser = Parser::from_source_code(&sess, FileName::Custom(name), src);

                let version = parser.parse_semver_version().map_err(|e| e.emit()).unwrap();
                assert_eq!(version.to_string(), v);
                let req = parser.parse_semver_req().map_err(|e| e.emit()).unwrap();
                assert_eq!(req.matches(&version), res, "v={v:?}, req={req_s:?}");
            }
        })
        .unwrap()
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
