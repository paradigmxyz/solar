#![allow(dead_code)]

use crate::ty::Gcx;

pub(crate) mod members;
pub use members::{Member, MemberMap};

macro_rules! declare_builtins {
    (|$gcx:ident| $($ns_name:ident { $($variant_name:ident => $key:ident : $value:expr;)* })*) => { paste::paste! {
        #[allow(non_snake_case)]
        #[allow(unused_variables)]
        mod builtin_members {
            use super::*;
            use super::members::*;
            use solar_interface::{sym::*, kw::*};
            use solar_ast::ast::StateMutability as SM;

            $(
                pub(crate) fn $ns_name(gcx: Gcx<'_>) -> MemberMapOwned<'_> {
                    [<$ns_name _iter>](gcx).collect()
                }

                pub(crate) fn [<$ns_name _iter>]($gcx: Gcx<'_>) -> impl Iterator<Item = Member<'_>> {
                    [$( Member::new($key, $value), )*].into_iter()
                }
            )*
        }
    }};
}

declare_builtins! {
    |gcx|

    global {
        Blockhash =>
            Blockhash: gcx.mk_builtin_fn(&[gcx.types.uint(256)], SM::View, &[gcx.types.fixed_bytes(32)]);
        Blobhash =>
            Blobhash: gcx.mk_builtin_fn(&[gcx.types.uint(256)], SM::View, &[gcx.types.fixed_bytes(32)]);

        Assert    => assert: gcx.mk_builtin_fn(&[gcx.types.bool], SM::Pure, &[]);
        Require   => require: gcx.mk_builtin_fn(&[gcx.types.bool], SM::Pure, &[]);
        Revert    => Revert: gcx.mk_builtin_fn(&[], SM::Pure, &[]);
        RevertMsg => Revert: gcx.mk_builtin_fn(&[gcx.types.string], SM::Pure, &[]);

        AddMod => Addmod: gcx.mk_builtin_fn(&[gcx.types.uint(256), gcx.types.uint(256), gcx.types.uint(256)], SM::Pure, &[gcx.types.uint(256)]);
        MulMod => Mulmod: gcx.mk_builtin_fn(&[gcx.types.uint(256), gcx.types.uint(256), gcx.types.uint(256)], SM::Pure, &[gcx.types.uint(256)]);

        Keccak256 => Keccak256: gcx.mk_builtin_fn(&[gcx.types.bytes_ref.memory], SM::View, &[gcx.types.fixed_bytes(32)]);
        Sha256    => sha256: gcx.mk_builtin_fn(&[gcx.types.bytes_ref.memory], SM::View, &[gcx.types.fixed_bytes(32)]);
        Ripemd160 => ripemd160: gcx.mk_builtin_fn(&[gcx.types.bytes_ref.memory], SM::View, &[gcx.types.fixed_bytes(20)]);
        EcRecover => ecrecover: gcx.mk_builtin_fn(&[gcx.types.fixed_bytes(32), gcx.types.uint(8), gcx.types.fixed_bytes(32), gcx.types.fixed_bytes(32)], SM::View, &[gcx.types.address]);
    }

    block {
        BlockCoinbase    => Coinbase: gcx.types.address_payable;
        BlockTimestamp   => Timestamp: gcx.types.uint(256);
        BlockDifficulty  => Difficulty: gcx.types.uint(256);
        BlockPrevrandao  => Prevrandao: gcx.types.uint(256);
        BlockNumber      => Number: gcx.types.uint(256);
        BlockGaslimit    => Gaslimit: gcx.types.uint(256);
        BlockChainid     => Chainid: gcx.types.uint(256);
        BlockBasefee     => Basefee: gcx.types.uint(256);
        BlockBlobbasefee => Blobbasefee: gcx.types.uint(256);
    }

    msg {
        MsgSender => sender: gcx.types.address;
        MsgGas    => Gas: gcx.types.uint(256);
        MsgValue  => value: gcx.types.uint(256);
        MsgData   => data: gcx.types.bytes_ref.calldata;
        MsgSig    => sig: gcx.types.fixed_bytes(4);
    }

    tx {
        TxOrigin   => Origin: gcx.types.address;
        TxGasPrice => Gasprice: gcx.types.uint(256);
    }

    abi {
        // TODO: `(T...) pure returns(bytes memory)`
        // encode: ;
        // TODO: `(T...) pure returns(bytes memory)`
        // encodePacked: ;
        // TODO: `(bytes4, T...) pure returns(bytes memory)`
        // encodeWithSelector: ;
        // TODO: `(F, T...) pure returns(bytes memory)`
        // encodeCall: ;
        // TODO: `(string memory, T...) pure returns(bytes memory)`
        // encodeWithSignature: ;
        // TODO: `(bytes memory, (T...)) pure returns(T...)`
        // decode: ;
    }

    // --- impls ---

    String {
        // TODO: `(string memory...) pure returns(string memory)`
        // concat: ;
    }

    Bytes {
        // TODO: `(bytes memory...) pure returns(bytes memory)`
        // concat: ;
    }

    address {
        AddressBalance      => Balance: gcx.types.uint(256);
        AddressCode         => code: gcx.types.bytes_ref.memory;
        AddressCodehash     => codehash: gcx.types.fixed_bytes(32);
        AddressCall         => Call: gcx.mk_builtin_fn(&[gcx.types.bytes_ref.memory], SM::View, &[gcx.types.bytes_ref.memory]);
        AddressDelegatecall => Delegatecall: gcx.mk_builtin_fn(&[gcx.types.bytes_ref.memory], SM::View, &[gcx.types.bytes_ref.memory]);
        AddressStaticcall   => Staticcall: gcx.mk_builtin_fn(&[gcx.types.bytes_ref.memory], SM::View, &[gcx.types.bytes_ref.memory]);
    }

    _address_payable {
        AddressPayableTransfer => transfer: gcx.mk_builtin_fn(&[gcx.types.uint(256)], SM::NonPayable, &[]);
        AddressPayableSend     => send: gcx.mk_builtin_fn(&[gcx.types.uint(256)], SM::NonPayable, &[gcx.types.bool]);
    }

    fixed_bytes {
        FixedBytesLength => length: gcx.types.uint(8);
    }

    array {
        ArrayLength => length: gcx.types.uint(256);
    }

    error {
        ErrorSelector => selector: gcx.types.fixed_bytes(4);
    }

    event {
        EventSelector => selector: gcx.types.fixed_bytes(32);
    }

    type_contract {
        ContractCreationCode => creationCode: gcx.types.bytes_ref.memory;
        ContractRuntimeCode  => runtimeCode: gcx.types.bytes_ref.memory;
        ContractName         => name: gcx.types.string_ref.memory;
    }

    type_interface {
        InterfaceId   => interfaceId: gcx.types.fixed_bytes(4);
        ContractName  => name: gcx.types.string_ref.memory;
    }
}
