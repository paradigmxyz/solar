//@compile-flags: -Zdump=mir
//@filecheck: --check-prefix=CDSFS

// Slicing a dynamic field of a calldata struct, the ERC-4337
// `PackedUserOperation` accessor shape. The struct stays in calldata and field
// reads decode only the selected head.
//
// A dynamic field remains a calldata slice, so it may be sliced again or have
// its `.offset` read in assembly.
// Verified against solc on anvil.

struct PackedUserOperation {
    address sender;
    uint256 nonce;
    bytes initCode;
    bytes callData;
    bytes32 accountGasLimits;
    uint256 preVerificationGas;
    bytes32 gasFees;
    bytes signature;
}

library ERC4337Utils {
    // Converting a slice to `bytesN` reads its leading word; the slice itself
    // must not survive into the backend.
    function factory(PackedUserOperation calldata self) internal pure returns (address) {
        return self.initCode.length < 20 ? address(0) : address(bytes20(self.initCode[0:20]));
    }

    // Hashing the memory slice reads the existing copy directly.
    function tailHash(PackedUserOperation calldata self) internal pure returns (bytes32) {
        return self.initCode.length < 20 ? bytes32(0) : keccak256(self.initCode[20:]);
    }

    function midWord(PackedUserOperation calldata self) internal pure returns (bytes32) {
        return bytes32(self.callData[0:32]);
    }
}

contract CalldataStructFieldSlice {
    // The single-use accessor is inlined into its wrapper. Past the length check, the
    // `bytes20` conversion loads one calldata word at the slice start and masks it.
    // CDSFS-LABEL: fn @factory{{[.][0-9]+}}
    // CDSFS-NOT: icall
    // CDSFS: lt {{v[0-9]+}}, 20
    // CDSFS: [[LEAD:v[0-9]+]] = calldataload
    // CDSFS-NEXT: [[MASKED:v[0-9]+]] = and [[LEAD]], 0xffffffffffffffffffffffffffffffffffffffff000000000000000000000000
    // CDSFS-NEXT: shr 96, [[MASKED]]
    function factory(PackedUserOperation calldata op) external pure returns (address) {
        return ERC4337Utils.factory(op);
    }

    // The single-use slice wrapper and hashing helper are consumed into the
    // wrapper entry: past the bounds checks, the `[20:]` slice is copied into
    // memory and the hash reads that copy directly.
    // CDSFS-LABEL: fn @tailHash{{[.][0-9]+}}
    // CDSFS: calldatacopy [[COPY:v[0-9]+]], {{v[0-9]+}}, [[LEN:v[0-9]+]]
    // CDSFS: keccak256 [[COPY]], [[LEN]]
    function tailHash(PackedUserOperation calldata op) external pure returns (bytes32) {
        return ERC4337Utils.tailHash(op);
    }

    // The `bytes32` conversion loads the word at the slice start once bounds are checked.
    // CDSFS-LABEL: fn @midWord{{[.][0-9]+}}
    // CDSFS: = lt {{v[0-9]+}}, 32
    // CDSFS: [[WORD:v[0-9]+]] = calldataload
    // CDSFS-NEXT: mstore 128, [[WORD]]
    function midWord(PackedUserOperation calldata op) external pure returns (bytes32) {
        return ERC4337Utils.midWord(op);
    }
}
