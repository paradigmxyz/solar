use crate::hir;
use solar_ast as ast;
use solar_data_structures::BumpExt;

impl<'gcx> super::super::LoweringContext<'gcx> {
    /// Lowers documentation comments from AST to HIR.
    ///
    /// Validation happens after parameters are lowered.
    pub(super) fn lower_item_docs(
        &mut self,
        item: &'gcx ast::Item<'gcx>,
        item_id: hir::ItemId,
    ) -> hir::DocId {
        if item.docs.is_empty() {
            return hir::DocId::EMPTY;
        }
        let docs = self.copy_doc_comments(&item.docs);
        self.lower_docs(docs, item_id)
    }

    fn copy_doc_comments(&self, docs: &ast::DocComments<'_>) -> ast::DocComments<'gcx> {
        let docs = docs.iter().map(|doc| ast::DocComment {
            kind: doc.kind,
            span: doc.span,
            symbol: doc.symbol,
            natspec: self.arena.bump().alloc_thin_slice_copy((), doc.natspec),
        });
        self.arena.bump().alloc_from_iter_thin((), docs).into()
    }

    fn lower_docs(&mut self, docs: ast::DocComments<'gcx>, item_id: hir::ItemId) -> hir::DocId {
        if docs.is_empty() {
            return hir::DocId::EMPTY;
        }

        self.hir.docs.push(hir::Doc {
            source: self.current_source_id,
            item: item_id,
            ast_comments: docs,
            comments: &[],
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::Compiler;
    use solar_interface::{ColorChoice, Session, sym};
    use std::path::PathBuf;

    #[test]
    fn natspec_inheritdoc_merges_tags() {
        use crate::hir::NatSpecKind;

        let src = r#"
contract Base {
    /**
     * @notice Base function notice
     * @dev Base function dev
     * @param x The x parameter from base
     * @param y The y parameter from base
     * @return success Whether the operation succeeded
     * @return value The result value
     * @custom:security Audited by Base team
     **/
    function foo(uint x, uint y) public virtual returns (bool success, uint value) {
        return (true, x + y);
    }
}

contract Child1 is Base {
    /// @inheritdoc Base
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) {
        return (true, x * y);
    }
}

contract Child2 is Base {
    /// @notice Child2 notice - overrides base
    /// @dev Child2 dev - overrides base
    /// @inheritdoc Base
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) {
        return (false, 0);
    }
}

contract Child3 is Base {
    /**
     * @param x The x parameter from child3
     * @inheritdoc Base
     **/
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) {
        return (true, x);
    }
}

contract Child4 is Base {
    /// @return success Child4 override for success
    /// @inheritdoc Base
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) {
        return (false, x + y);
    }
}

contract Child5 is Base {
    /**
    * @custom:audit Reviewed by Child5 auditor
    * @inheritdoc Base
    **/
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) {
        return (true, y);
    }
}

contract GrandChild is Child1 {
    /// @inheritdoc Child1
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) {
        return (true, x - y);
    }
}

"#;
        let compiler = lower_source(src);
        compiler.enter_sequential(|c| {
            let gcx = c.gcx();

            let get_comments = |contract_name: &str, func_name: &str| {
                gcx.hir
                    .functions()
                    .find(|f| {
                        f.contract.is_some_and(|cid| {
                            gcx.hir.contract(cid).name.as_str() == contract_name
                                && f.name.is_some_and(|n| n.as_str() == func_name)
                        })
                    })
                    .map(|func| gcx.hir.doc(func.doc).comments())
                    .unwrap_or_else(|| panic!("{contract_name}.{func_name} not found"))
            };
            let base = get_comments("Base", "foo");
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Notice)), 1);
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Dev)), 1);
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Param { .. })), 2);
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Return { .. })), 2);
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Custom { .. })), 1);
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Notice),
                "Base function notice",
                "Base @notice",
            );
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Dev),
                "Base function dev",
                "Base @dev",
            );
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Param { name } if name.name == sym::x),
                "x parameter from base",
                "Base @param x",
            );
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Param { name } if name.name.as_str() == "y"),
                "y parameter from base",
                "Base @param y",
            );
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "success"),
                "operation succeeded",
                "Base @return success",
            );
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "value"),
                "result value",
                "Base @return value",
            );
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Custom { name } if name.name.as_str() == "security"),
                "Audited by Base team",
                "Base @custom:security",
            );

            let c = get_comments("Child1", "foo");
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Notice)), 1);
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Dev)), 1);
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Param { .. })), 2);
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Return { .. })), 2);
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Custom { .. })), 1);

            let c = get_comments("Child2", "foo");
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Notice),
                "Child2 notice",
                "Child2 @notice",
            );
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Dev),
                "Child2 dev",
                "Child2 @dev",
            );
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Param { .. })), 2);

            let c = get_comments("Child3", "foo");
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Param { name } if name.name == sym::x),
                "from child3",
                "Child3 @param x",
            );
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Param { name } if name.name.as_str() == "y"),
                "from base",
                "Child3 @param y",
            );

            let c = get_comments("Child4", "foo");
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "success"),
                "Child4 override",
                "Child4 @return success",
            );
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "value"),
                "result value",
                "Child4 @return value",
            );

            let c = get_comments("Child5", "foo");
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Custom { .. })), 2);
            assert!(
                c.iter().any(
                    |i| matches!(i.kind, NatSpecKind::Custom { name } if name.name.as_str() == "security")
                )
            );
            assert!(
                c.iter().any(
                    |i| matches!(i.kind, NatSpecKind::Custom { name } if name.name.as_str() == "audit")
                )
            );

            let g = get_comments("GrandChild", "foo");
            assert_tag_contains(
                g,
                |k| matches!(k, NatSpecKind::Notice),
                "Base function notice",
                "GrandChild @notice",
            );
            assert_eq!(count_tags(g, |k| matches!(k, NatSpecKind::Param { .. })), 2);

        });
    }

    #[test]
    fn natspec_inheritdoc_matches_overload_signature() {
        use crate::hir::NatSpecKind;

        let src = r#"
contract OverloadBase {
    /// @notice address overload
    /// @param a address parameter
    function overloaded(address a) public virtual {}

    /// @notice uint overload
    /// @param a uint parameter
    function overloaded(uint a) public virtual {}
}

contract OverloadChild is OverloadBase {
    /// @inheritdoc OverloadBase
    function overloaded(uint a) public override {}
}
"#;
        let compiler = lower_source(src);
        compiler.enter_sequential(|c| {
            let overloaded = function_comments(c.gcx(), "OverloadChild", "overloaded");
            assert_tag_contains(
                overloaded,
                |k| matches!(k, NatSpecKind::Notice),
                "uint overload",
                "OverloadChild @notice",
            );
            assert_tag_contains(
                overloaded,
                |k| matches!(k, NatSpecKind::Param { name } if name.name.as_str() == "a"),
                "uint parameter",
                "OverloadChild @param a",
            );
            assert!(
                !overloaded.iter().any(|item| item.content().contains("address")),
                "OverloadChild inherited docs from the wrong overload"
            );
        });
    }

    #[test]
    fn natspec_public_variable_return_docs() {
        use crate::hir::NatSpecKind;

        let src = r#"
contract VariableDocs {
    /// @return The number of decimals
    uint8 public decimals;
}
"#;
        let compiler = lower_source(src);
        compiler.enter_sequential(|c| {
            let decimals = variable_comments(c.gcx(), "VariableDocs", "decimals");
            assert_tag_contains(
                decimals,
                |k| matches!(k, NatSpecKind::Return { name: None }),
                "The number of decimals",
                "VariableDocs.decimals @return",
            );
        });
    }

    #[test]
    fn natspec_return_docs_resolve_names() {
        use crate::hir::NatSpecKind;

        let src = r#"
contract ReturnDocs {
    /// @return the value
    function unnamedReturn() public pure returns (uint) {
        return 1;
    }

    /// @return result The value
    function namedReturn() public pure returns (uint result) {
        return 1;
    }
}
"#;
        let compiler = lower_source(src);
        compiler.enter_sequential(|c| {
            let unnamed = function_comments(c.gcx(), "ReturnDocs", "unnamedReturn");
            assert_tag_contains(
                unnamed,
                |k| matches!(k, NatSpecKind::Return { name: None }),
                "the value",
                "ReturnDocs.unnamedReturn @return",
            );

            let named = function_comments(c.gcx(), "ReturnDocs", "namedReturn");
            assert_tag_contains(
                named,
                |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "result"),
                "The value",
                "ReturnDocs.namedReturn @return result",
            );
        });
    }

    #[test]
    fn statement_variables_preserve_function_parent() {
        let src = r#"
contract LocalParent {
    function locals() public pure {
        uint a = 1;
        (uint b, uint c) = (2, 3);
    }
}
"#;
        let compiler = lower_source(src);
        compiler.enter_sequential(|c| {
            let gcx = c.gcx();
            let locals = function_id(gcx, "LocalParent", "locals");
            let local_parent = Some(crate::hir::ItemId::Function(locals));
            let local_names: Vec<_> = gcx
                .hir
                .variables()
                .filter(|v| v.kind == crate::hir::VarKind::Statement)
                .filter_map(|v| v.name.map(|name| (name.name, v.parent)))
                .collect();
            for name in ["a", "b", "c"] {
                assert!(
                    local_names.iter().any(|&(local, parent)| {
                        local.as_str() == name && parent == local_parent
                    }),
                    "local variable {name} did not preserve its function parent"
                );
            }
        });
    }

    fn function_id(
        gcx: crate::ty::Gcx<'_>,
        contract_name: &str,
        func_name: &str,
    ) -> crate::hir::FunctionId {
        gcx.hir
            .functions_enumerated()
            .find(|(_, f)| {
                f.contract.is_some_and(|cid| {
                    gcx.hir.contract(cid).name.as_str() == contract_name
                        && f.name.is_some_and(|n| n.as_str() == func_name)
                })
            })
            .map(|(id, _)| id)
            .unwrap_or_else(|| panic!("{contract_name}.{func_name} not found"))
    }

    fn function_comments<'gcx>(
        gcx: crate::ty::Gcx<'gcx>,
        contract_name: &str,
        func_name: &str,
    ) -> &'gcx [crate::hir::NatSpecItem] {
        let id = function_id(gcx, contract_name, func_name);
        gcx.hir.doc(gcx.hir.function(id).doc).comments()
    }

    fn variable_comments<'gcx>(
        gcx: crate::ty::Gcx<'gcx>,
        contract_name: &str,
        var_name: &str,
    ) -> &'gcx [crate::hir::NatSpecItem] {
        gcx.hir
            .variables()
            .find(|v| {
                v.contract.is_some_and(|cid| {
                    gcx.hir.contract(cid).name.as_str() == contract_name
                        && v.name.is_some_and(|n| n.as_str() == var_name)
                })
            })
            .and_then(|var| var.doc.map(|doc| gcx.hir.doc(doc).comments()))
            .unwrap_or_else(|| panic!("{contract_name}.{var_name} not found"))
    }

    fn lower_source(src: &str) -> Compiler {
        let sess =
            Session::builder().with_buffer_emitter(ColorChoice::Never).single_threaded().build();
        let mut compiler = Compiler::new(sess);

        let _ = compiler.enter_mut(|compiler| -> solar_interface::Result<_> {
            let mut parsing_context = compiler.parse();
            let file = compiler
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("test.sol"), src.to_string())
                .unwrap();
            parsing_context.add_file(file);
            parsing_context.parse();
            let _ = compiler.lower_asts()?;
            let _ = compiler.analysis()?;
            Ok(())
        });

        compiler
    }

    fn count_tags(
        comments: &[crate::hir::NatSpecItem],
        kind: fn(&crate::hir::NatSpecKind) -> bool,
    ) -> usize {
        comments.iter().filter(|i| kind(&i.kind)).count()
    }

    fn assert_tag_contains(
        comments: &[crate::hir::NatSpecItem],
        kind_matcher: fn(&crate::hir::NatSpecKind) -> bool,
        expected_content: &str,
        msg: &str,
    ) {
        let tag = comments.iter().find(|i| kind_matcher(&i.kind)).expect(msg);
        assert!(tag.content().contains(expected_content), "{}: Got: {}", msg, tag.content());
    }
}
