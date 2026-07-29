//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=CDSFS

// Slicing a dynamic field of a calldata struct, the ERC-4337
// `PackedUserOperation` accessor shape. The prologue rebuilds a calldata struct
// in memory, including independent memory objects for its dynamic members, and
// ordinary reads of a member go through that copy.
//
// Slicing is the exception: the result keeps a calldata-located type, so it may
// be sliced again or have its `.offset` read in assembly, neither of which the
// copy can answer. The copy carries the struct's calldata position in a
// trailing word, so the slice is taken there instead.
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
    // CDSFS-LABEL: fn @factory
    // CDSFS: calldataload
    // CDSFS-NOT: make_calldata_slice
    function factory(PackedUserOperation calldata op) external pure returns (address) {
        return ERC4337Utils.factory(op);
    }

    // CDSFS-LABEL: fn @tailHash
    // CDSFS: keccak256
    function tailHash(PackedUserOperation calldata op) external pure returns (bytes32) {
        return ERC4337Utils.tailHash(op);
    }

    // CDSFS-LABEL: fn @midWord
    function midWord(PackedUserOperation calldata op) external pure returns (bytes32) {
        return ERC4337Utils.midWord(op);
    }
}
