use crate::hir;

macro_rules! declare_builtins {
    ($($t:tt)*) => {};
}

declare_builtins! {
    |tys| {
        let uint = tys.uint(256);
    }

    // TODO: `(uint256) view returns(bytes32)`
    // blockhash: ;
    // TODO: `(uint256) view returns(bytes32)`
    // blobhash: ;

    // TODO: `(bool) pure`
    // assert: ;
    // TODO: `(bool) pure`
    // require: ;
    // TODO: `() pure`
    // revert: ;
    // TODO: `(string memory) pure`
    // revert: ;

    // TODO: `(uint256, uint256, uint256) pure returns(uint256)`
    // addmod: ;
    // TODO: `(uint256, uint256, uint256) pure returns(uint256)`
    // mulmod: ;

    // TODO: `(bytes memory) pure returns(bytes32)`
    // keccak256: ;
    // TODO: `(bytes memory) pure returns(bytes32)`
    // sha256: ;
    // TODO: `(bytes memory) pure returns(bytes20)`
    // ripemd160: ;
    // TODO: `(bytes32, uint8, bytes32, bytes32) returns (address)`
    // ecrecover: ;

    // TODO:

    ns block {
        coinbase: tys.address_payable;
        timestamp: uint;
        difficulty: uint;
        prevrandao: uint;
        number: uint;
        gaslimit: uint;
        chainid: uint;
        basefee: uint;
        blobbasefee: uint;
    }

    ns msg {
        sender: tys.address;
        gas: uint;
        value: uint;
        data: tys.bytes_ref.calldata;
        sig: tys.fixed_bytes(4);
    }

    ns tx {
        origin: tys.address;
        gasprice: uint;
    }

    ns abi {
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

    impl bytes {
        // TODO: `(bytes memory...) pure returns(bytes memory)`
        // concat: ;
    }

    impl string {
        // TODO: `(string memory...) pure returns(string memory)`
        // concat: ;
    }

    impl address {
        balance: uint;
        code: tys.bytes_ref.memory;
        codehash: tys.fixed_bytes(32);
        // TODO: `(bytes memory) view returns(bool, bytes memory)`
        // call: ;
        // TODO: `(bytes memory) view returns(bool, bytes memory)`
        // delegatecall: ;
        // TODO: `(bytes memory) view returns(bool, bytes memory)`
        // staticcall: ;
    }

    impl address_payable {
        // TODO: inherit `address`
        // TODO: `(uint256)`
        // transfer: ;
        // TODO: `(uint256) returns(bool)`
        // send: ;
    }

    // TODO: `type(T)` members like min, max
}

pub fn inject_builtins(_hir: &mut hir::Hir<'_>) {}
