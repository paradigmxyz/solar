//@ revisions: homestead osaka
//@[homestead] compile-flags: -O none --evm-version homestead --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD
//@[osaka] compile-flags: -O none --evm-version osaka --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@[osaka] filecheck: --check-prefix=OSAKA

// A storage reference crossing a library call boundary travels as its slot number in both
// directions, like solc's `ArrayType::encodingType`, so the ABI wrapper returns one word and the
// caller decodes one word. Storage pointers are a single static word, so the call is legal before
// Byzantium too. `run-call` cannot deploy an unlinked library, so this pins the lowering only.
library Lib {
    struct Plain {
        uint256 a;
    }

    // The wrapper receives the slot and returns the slot, never a memory array.
    // HOMESTEAD: fn @arrRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    // OSAKA: fn @arrRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    function arrRef(uint256[] storage a) external pure returns (uint256[] storage) {
        return a;
    }

    // HOMESTEAD: fn @bytesRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    // OSAKA: fn @bytesRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    function bytesRef(bytes storage b) external pure returns (bytes storage) {
        return b;
    }

    // HOMESTEAD: fn @plainRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    // OSAKA: fn @plainRef{{.*}}abi_params=[storageptr], abi_returns=[word]
    function plainRef(Plain storage p) external pure returns (Plain storage) {
        return p;
    }

    // A slot next to a value keeps the value's own shape.
    // HOMESTEAD: fn @pairRef{{.*}}abi_returns=[word, word]
    // OSAKA: fn @pairRef{{.*}}abi_returns=[word, word]
    function pairRef(uint256[] storage a) external view returns (uint256, uint256[] storage) {
        return (a.length, a);
    }
}

contract C {
    using Lib for uint256[];

    uint256[] private nums;
    bytes private bs;
    Lib.Plain private plain;

    // Before Byzantium the returned slot comes out of the delegatecall's own 32-byte output area,
    // as solc's `delegatecall(..., out, 32)` does; from Byzantium on it is decoded out of the
    // return data as a word and used as a slot.
    // HOMESTEAD-LABEL: fn @len
    // HOMESTEAD: delegatecall {{.*}}, 0, 32
    // HOMESTEAD: mload
    // HOMESTEAD: sload
    // OSAKA-LABEL: fn @len
    // OSAKA: delegatecall {{.*}}, 0, 0
    // OSAKA: [[SLOT:v[0-9]+]] = abi_decode [storageptr]
    // OSAKA: sload [[SLOT]]
    function len() external view returns (uint256) {
        return Lib.arrRef(nums).length;
    }

    // A `bytes` slot is a slot too: the length still comes from storage.
    // HOMESTEAD-LABEL: fn @bytesLen
    // HOMESTEAD: delegatecall {{.*}}, 0, 32
    // HOMESTEAD: mload
    // HOMESTEAD: sload
    // OSAKA-LABEL: fn @bytesLen
    // OSAKA: [[SLOT:v[0-9]+]] = abi_decode [storageptr]
    // OSAKA: sload [[SLOT]]
    function bytesLen() external view returns (uint256) {
        return Lib.bytesRef(bs).length;
    }

    // A struct pointer indexes its member off the returned slot.
    // HOMESTEAD-LABEL: fn @member
    // HOMESTEAD: delegatecall {{.*}}, 0, 32
    // HOMESTEAD: mload
    // HOMESTEAD: sload
    // OSAKA-LABEL: fn @member
    // OSAKA: [[SLOT:v[0-9]+]] = abi_decode [storageptr]
    // OSAKA: sload [[SLOT]]
    function member() external view returns (uint256) {
        return Lib.plainRef(plain).a;
    }

    // Two return words share one output area, and the second one is the slot.
    // HOMESTEAD-LABEL: fn @pair
    // HOMESTEAD: delegatecall {{.*}}, 64
    // OSAKA-LABEL: fn @pair
    // OSAKA: abi_decode [u256, storageptr]
    function pair() external view returns (uint256, uint256) {
        (uint256 n, uint256[] storage r) = Lib.pairRef(nums);
        return (n, r.length);
    }

    // An attached call passes the receiver's slot and gets a slot back.
    // HOMESTEAD-LABEL: fn @attached
    // HOMESTEAD: delegatecall {{.*}}, 0, 32
    // HOMESTEAD: mload
    // HOMESTEAD: sload
    // OSAKA-LABEL: fn @attached
    // OSAKA: [[SLOT:v[0-9]+]] = abi_decode [storageptr]
    // OSAKA: sload [[SLOT]]
    function attached() external view returns (uint256) {
        return nums.arrRef().length;
    }
}
