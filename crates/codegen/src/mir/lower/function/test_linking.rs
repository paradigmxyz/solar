//! Lowers explicitly selected test bytecode references to Foundry's artifact cheatcodes.
//!
//! Selection is supplied by the build tool, which owns dependency invalidation and decides
//! which references can use the test runtime. Ordinary deployments and production artifacts
//! keep native bytecode. Constructor arguments use the same typed lowering and ABI encoder
//! as native construction; no generated Solidity helpers or rewritten source are needed.

use super::*;
use alloy_primitives::address;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_test_deployment(
        &mut self,
        span: Span,
        contract: hir::ContractId,
        args: ValueId,
        value: ValueId,
        salt: Option<ValueId>,
    ) -> Option<ValueId> {
        // input = abi_encode(selector, artifact, args, value[, salt])
        let artifact = self.test_artifact_name(contract)?;
        let mut values = vec![artifact, args, value];
        let mut types = vec![
            AbiType::Bytes(SliceLocation::Memory),
            AbiType::Bytes(SliceLocation::Memory),
            AbiType::Word(None),
        ];
        let signature = if let Some(salt) = salt {
            values.push(salt);
            types.push(AbiType::Word(None));
            "deployCode(string,bytes,uint256,bytes32)"
        } else {
            "deployCode(string,bytes,uint256)"
        };
        self.call_test_cheatcode(span, signature, values, types, self.cx.gcx.types.address, false)
    }

    pub(super) fn lower_test_creation_code(
        &mut self,
        span: Span,
        contract: hir::ContractId,
    ) -> Option<ValueId> {
        // input = abi_encode(getCode.selector, artifact)
        // code = abi_decode(bytes, staticcall(vm, input))
        let artifact = self.test_artifact_name(contract)?;
        self.call_test_cheatcode(
            span,
            "getCode(string)",
            vec![artifact],
            vec![AbiType::Bytes(SliceLocation::Memory)],
            self.cx.gcx.types.bytes_ref.memory,
            true,
        )
    }

    fn test_artifact_name(&mut self, contract: hir::ContractId) -> Option<ValueId> {
        let contract = self.cx.gcx.hir.contract(contract);
        let source = self.cx.gcx.hir.source(contract.source);
        let artifact = format!("{}:{}", source.file.name.display(), contract.name);
        // artifact = bytes(source_unit ++ ":" ++ contract_name)
        self.lower_bytes_literal(artifact.as_bytes())
    }

    fn call_test_cheatcode(
        &mut self,
        span: Span,
        signature: &str,
        values: Vec<ValueId>,
        types: Vec<AbiType>,
        return_type: Ty<'gcx>,
        is_static: bool,
    ) -> Option<ValueId> {
        if !self.cx.gcx.sess.opts.evm_version.supports_returndata() {
            return self.cx.report_unsupported(span, "test linking before Byzantium");
        }
        // input = abi_encode(selector, args)
        let hash = keccak256(signature.as_bytes());
        let selector = self.builder.imm(U256::from_be_slice(&hash[..4]));
        let layout = Arc::new(AbiLayout::new(types));
        let input = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let ptr = self.builder.slice_ptr(input);
        let len = self.builder.slice_len(input);
        let vm = self.builder.imm(U256::from_be_slice(
            address!("7109709ECfa91a80626fF3989D68f67F5b1DD12D").as_slice(),
        ));
        let zero = self.builder.imm(U256::ZERO);
        // ok = call|staticcall(gas(), vm, input, 0, 0)
        // if !ok { revert(returndata) }
        let gas = self.builder.gas();
        let success = if is_static {
            self.builder.staticcall(gas, vm, ptr, len, zero, zero)
        } else {
            self.builder.call(gas, vm, zero, ptr, len, zero, zero)
        };
        self.revert_external_call(success);
        // value = abi_decode(return_type, returndata)
        let data = self.materialize_returndata_bytes();
        self.lower_abi_decode_values(data, &[return_type], span)?.into_iter().next()
    }
}
