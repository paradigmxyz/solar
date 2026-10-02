//! Renders builtin signatures from the type selected by semantic analysis.
//!
//! Hover signatures retain their receiver's concrete types, including user-defined value types
//! and bound array operations. Official documentation links are selected during analysis and
//! rendered without filesystem or network access when a hover is requested.

use lsp_types::{MarkupContent, MarkupKind};
use solar_sema::{
    Gcx,
    builtins::Builtin,
    hir::{self, StateMutability},
    ty::{Ty, TyKind},
};
use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BuiltinDocumentation {
    builtin: Builtin,
    signature: String,
    description: &'static str,
    reference: &'static str,
}

impl BuiltinDocumentation {
    pub(crate) fn for_expr<'gcx>(
        gcx: Gcx<'gcx>,
        expr: &hir::Expr<'gcx>,
        builtin: Builtin,
    ) -> Option<Self> {
        let expr = expr.peel_parens();
        // Several receiver-dependent builtins deliberately have no standalone builtin type.
        let ty = gcx.type_of_expr(expr.id)?;
        if ty.references_error() {
            return None;
        }
        let name = builtin_name(gcx, expr, builtin);
        let signature = match ty.kind {
            TyKind::BuiltinModule(_) => format!("namespace {name}"),
            TyKind::Fn(function) => {
                let mut parameters = function.parameters;
                // Called members already have the implicit receiver removed by type checking.
                // A member value can still carry its unbound storage-array parameter.
                if function.attached
                    || (gcx.resolved_callee(expr.id).is_none()
                        && matches!(
                            builtin,
                            Builtin::ArrayPush0 | Builtin::ArrayPush | Builtin::ArrayPop
                        ))
                {
                    parameters = parameters.get(1..)?;
                }
                let mut signature = format!("function {name}");
                append_types(&mut signature, gcx, parameters);
                if function.state_mutability != StateMutability::NonPayable {
                    write!(signature, " {}", function.state_mutability).unwrap();
                }
                if !function.returns.is_empty() {
                    signature.push_str(" returns ");
                    append_types(&mut signature, gcx, function.returns);
                }
                signature
            }
            _ => format!("{} {name}", ty.display(gcx)),
        };
        Some(Self {
            builtin,
            signature,
            description: description(builtin),
            reference: reference(gcx, expr, builtin),
        })
    }

    pub(crate) fn hover(&self) -> MarkupContent {
        MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!(
                "```solidity\n{}\n```\n\n{}\n\n\
                 *Compiler-provided builtin; no Solidity source declaration.*\n\n\
                 [Solidity documentation](https://docs.soliditylang.org/en/latest/{})",
                self.signature, self.description, self.reference,
            ),
        }
    }
}

fn append_types<'gcx>(output: &mut String, gcx: Gcx<'gcx>, types: &[Ty<'gcx>]) {
    output.push('(');
    for (index, ty) in types.iter().enumerate() {
        if index != 0 {
            output.push_str(", ");
        }
        write!(output, "{}", ty.display(gcx)).unwrap();
    }
    output.push(')');
}

fn builtin_name<'gcx>(gcx: Gcx<'gcx>, expr: &hir::Expr<'gcx>, builtin: Builtin) -> String {
    let name = builtin.name();
    if let hir::ExprKind::Member(receiver, _) = expr.kind
        && let Some(receiver_ty) = gcx.type_of_expr(receiver.id)
    {
        match receiver_ty.kind {
            TyKind::BuiltinModule(module) => return format!("{}.{name}", module.name()),
            TyKind::Type(inner) => return format!("{}.{name}", inner.display(gcx)),
            TyKind::Meta(inner) => return format!("type({}).{name}", inner.display(gcx)),
            TyKind::Fn(_) => return format!("function.{name}"),
            TyKind::Error(..) => return format!("error.{name}"),
            TyKind::Event(..) => return format!("event.{name}"),
            _ => {}
        }
    }
    let receiver = match builtin {
        Builtin::AddressBalance
        | Builtin::AddressCode
        | Builtin::AddressCodehash
        | Builtin::AddressCall
        | Builtin::AddressDelegatecall
        | Builtin::AddressStaticcall => "address",
        Builtin::AddressPayableTransfer | Builtin::AddressPayableSend => "address payable",
        Builtin::ArrayLength | Builtin::ArrayPush0 | Builtin::ArrayPush | Builtin::ArrayPop => {
            "array"
        }
        Builtin::FixedBytesLength => "bytesN",
        _ => return name.to_string(),
    };
    format!("{receiver}.{name}")
}

fn description(builtin: Builtin) -> &'static str {
    match builtin {
        Builtin::Blockhash => "Returns the hash of a recent block, or zero when unavailable.",
        Builtin::Blobhash => {
            "Returns the versioned hash of a blob in the current transaction, or zero when the index is out of range."
        }
        Builtin::Gasleft => "Returns the gas remaining in the current call.",
        Builtin::Selfdestruct => {
            "Transfers the contract's balance to the recipient. Contract deletion depends on the executing chain's EVM rules and whether the contract was created in the same transaction."
        }
        Builtin::Assert => {
            "Reverts with a panic if the condition is false. Use it to check internal invariants."
        }
        Builtin::Require => {
            "Reverts if the condition is false, optionally providing an error message or custom error."
        }
        Builtin::Revert => "Aborts execution and reverts the current call's state changes.",
        Builtin::RevertMsg => {
            "Aborts execution and reverts the current call's state changes with an error message."
        }
        Builtin::AddMod => {
            "Computes (x + y) modulo k with arbitrary-precision intermediate arithmetic. The modulus must be nonzero."
        }
        Builtin::MulMod => {
            "Computes (x * y) modulo k with arbitrary-precision intermediate arithmetic. The modulus must be nonzero."
        }
        Builtin::Keccak256 => "Computes the Keccak-256 hash of the input bytes.",
        Builtin::Sha256 => "Computes the SHA-256 hash of the input bytes.",
        Builtin::Ripemd160 => "Computes the RIPEMD-160 hash of the input bytes.",
        Builtin::EcRecover => {
            "Recovers the address associated with an ECDSA signature, or returns the zero address on failure."
        }
        Builtin::Erc7201 => "Computes the ERC-7201 storage location for a namespace identifier.",
        Builtin::Block => "Provides information about the current block.",
        Builtin::Msg => "Provides information about the current message call.",
        Builtin::Tx => "Provides information about the current transaction.",
        Builtin::Abi => "Provides ABI encoding and decoding functions.",
        Builtin::This => "Refers to the current contract as an externally callable contract value.",
        Builtin::Super => {
            "Accesses inherited contract members according to the inheritance linearization."
        }
        Builtin::BlockCoinbase => "The current block's beneficiary address.",
        Builtin::BlockTimestamp => "The current block's timestamp in seconds since the Unix epoch.",
        Builtin::BlockDifficulty => {
            "The current block's difficulty on EVM versions before Paris; replaced by block.prevrandao from Paris onward."
        }
        Builtin::BlockPrevrandao => {
            "The current block's randomness value supplied by the consensus layer."
        }
        Builtin::BlockNumber => "The current block number.",
        Builtin::BlockGaslimit => "The current block's gas limit.",
        Builtin::BlockChainid => "The current chain ID.",
        Builtin::BlockBasefee => "The current block's base fee in wei per gas.",
        Builtin::BlockBlobbasefee => "The current block's blob base fee.",
        Builtin::BlockSlotnum => "The current block's consensus slot number.",
        Builtin::MsgSender => "The address of the sender of the current message call.",
        Builtin::MsgGas => "The gas remaining in the current call. Use gasleft() instead.",
        Builtin::MsgValue => "The amount of wei sent with the current message call.",
        Builtin::MsgData => "The complete calldata of the current message call.",
        Builtin::MsgSig => "The first four bytes of the current message call's calldata.",
        Builtin::TxOrigin => "The address that originated the transaction.",
        Builtin::TxGasPrice => "The transaction's effective gas price in wei per gas.",
        Builtin::AbiEncode => "ABI-encodes the arguments into a byte array.",
        Builtin::AbiEncodePacked => {
            "Encodes the arguments using the non-standard packed ABI encoding. Values may be ambiguous when multiple dynamic arguments are packed."
        }
        Builtin::AbiEncodeWithSelector => {
            "ABI-encodes the arguments after the given four-byte function selector."
        }
        Builtin::AbiEncodeWithSignature => {
            "ABI-encodes the arguments after the selector derived from the given function signature."
        }
        Builtin::AbiEncodeCall => {
            "ABI-encodes a call using the function's selector and a type-checked tuple of arguments."
        }
        Builtin::AbiDecode => "ABI-decodes the input bytes into the specified tuple of types.",
        Builtin::AddressBalance => "The account's balance in wei.",
        Builtin::AddressCode => "The bytecode stored at the account's address.",
        Builtin::AddressCodehash => {
            "The Keccak-256 hash of the account's code, subject to EVM account-existence rules."
        }
        Builtin::AddressCall => {
            "Calls the address with raw calldata and returns the success flag and return data."
        }
        Builtin::AddressDelegatecall => {
            "Calls code at the address in the current contract's context and returns the success flag and return data."
        }
        Builtin::AddressStaticcall => {
            "Calls the address while prohibiting state changes and returns the success flag and return data."
        }
        Builtin::AddressPayableTransfer => {
            "Sends wei to the address with a 2300 gas stipend and reverts if the transfer fails."
        }
        Builtin::AddressPayableSend => {
            "Sends wei to the address with a 2300 gas stipend and returns whether the transfer succeeded."
        }
        Builtin::FixedBytesLength => "The number of bytes in the fixed-size byte value.",
        Builtin::ArrayLength => "The number of elements in the array or bytes in the byte array.",
        Builtin::ArrayPush0 => {
            "Appends a zero-initialized element to the storage array and returns a reference to the new element."
        }
        Builtin::ArrayPush => "Appends the supplied element to the storage array.",
        Builtin::ArrayPop => {
            "Removes the final element from the storage array. Reverts if the array is empty."
        }
        Builtin::FunctionSelector => {
            "The four-byte selector of a function or custom error, derived from its canonical ABI signature."
        }
        Builtin::FunctionAddress => "The address associated with an external function value.",
        Builtin::EventSelector => {
            "The Keccak-256 hash of the event's canonical ABI signature, used as its signature topic."
        }
        Builtin::ContractCreationCode => {
            "The contract's creation bytecode, without ABI-encoded constructor arguments."
        }
        Builtin::ContractRuntimeCode => "The contract's runtime bytecode.",
        Builtin::ContractName => "The contract's name.",
        Builtin::InterfaceId => {
            "The ERC-165 interface identifier, computed by XORing the interface's function selectors."
        }
        Builtin::TypeMin => "The minimum value representable by the specified type.",
        Builtin::TypeMax => "The maximum value representable by the specified type.",
        Builtin::UdvtWrap => {
            "Converts a value of the underlying type into the user-defined value type without changing its representation."
        }
        Builtin::UdvtUnwrap => {
            "Converts a user-defined value type into its underlying type without changing its representation."
        }
        Builtin::StringConcat => "Concatenates the string arguments into a new string in memory.",
        Builtin::BytesConcat => "Concatenates the byte arguments into a new byte array in memory.",
        _ if builtin.is_yul() => {
            "An EVM builtin available in inline assembly. Parameters and results are 256-bit words."
        }
        _ => "A compiler-provided Solidity builtin.",
    }
}

fn reference(gcx: Gcx<'_>, expr: &hir::Expr<'_>, builtin: Builtin) -> &'static str {
    match builtin {
        Builtin::Blockhash
        | Builtin::Blobhash
        | Builtin::Gasleft
        | Builtin::Block
        | Builtin::Msg
        | Builtin::Tx
        | Builtin::BlockCoinbase
        | Builtin::BlockTimestamp
        | Builtin::BlockDifficulty
        | Builtin::BlockPrevrandao
        | Builtin::BlockNumber
        | Builtin::BlockGaslimit
        | Builtin::BlockChainid
        | Builtin::BlockBasefee
        | Builtin::BlockBlobbasefee
        | Builtin::BlockSlotnum
        | Builtin::MsgSender
        | Builtin::MsgGas
        | Builtin::MsgValue
        | Builtin::MsgData
        | Builtin::MsgSig
        | Builtin::TxOrigin
        | Builtin::TxGasPrice => "units-and-global-variables.html#block-and-transaction-properties",
        Builtin::Abi
        | Builtin::AbiEncode
        | Builtin::AbiEncodePacked
        | Builtin::AbiEncodeWithSelector
        | Builtin::AbiEncodeWithSignature
        | Builtin::AbiEncodeCall
        | Builtin::AbiDecode => {
            "units-and-global-variables.html#abi-encoding-and-decoding-functions"
        }
        Builtin::Assert | Builtin::Require | Builtin::Revert | Builtin::RevertMsg => {
            "units-and-global-variables.html#error-handling"
        }
        Builtin::AddMod
        | Builtin::MulMod
        | Builtin::Keccak256
        | Builtin::Sha256
        | Builtin::Ripemd160
        | Builtin::EcRecover
        | Builtin::Erc7201 => {
            "units-and-global-variables.html#mathematical-and-cryptographic-functions"
        }
        Builtin::This | Builtin::Selfdestruct => "units-and-global-variables.html#contract-related",
        Builtin::Super => "contracts.html#inheritance",
        Builtin::AddressBalance
        | Builtin::AddressCode
        | Builtin::AddressCodehash
        | Builtin::AddressCall
        | Builtin::AddressDelegatecall
        | Builtin::AddressStaticcall
        | Builtin::AddressPayableTransfer
        | Builtin::AddressPayableSend => "types.html#members-of-addresses",
        Builtin::FixedBytesLength => "types.html#fixed-size-byte-arrays",
        Builtin::ArrayLength | Builtin::ArrayPush0 | Builtin::ArrayPush | Builtin::ArrayPop => {
            "types.html#array-members"
        }
        Builtin::FunctionSelector => {
            if let hir::ExprKind::Member(receiver, _) = expr.kind
                && let Some(ty) = gcx.type_of_expr(receiver.id)
                && matches!(ty.kind, TyKind::Error(..))
            {
                "contracts.html#members-of-errors"
            } else {
                "types.html#function-types"
            }
        }
        Builtin::FunctionAddress => "types.html#function-types",
        Builtin::EventSelector => "contracts.html#members-of-events",
        Builtin::ContractCreationCode
        | Builtin::ContractRuntimeCode
        | Builtin::ContractName
        | Builtin::InterfaceId
        | Builtin::TypeMin
        | Builtin::TypeMax => "units-and-global-variables.html#type-information",
        Builtin::UdvtWrap | Builtin::UdvtUnwrap => "types.html#user-defined-value-types",
        Builtin::StringConcat => "types.html#string-concat",
        Builtin::BytesConcat => "types.html#bytes-concat",
        _ if builtin.is_yul() => "yul.html#evm-dialect",
        _ => "units-and-global-variables.html",
    }
}
