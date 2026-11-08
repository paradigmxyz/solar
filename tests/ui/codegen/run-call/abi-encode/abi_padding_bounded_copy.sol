//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: check 0x112233, 7, 3 => 293
//@ run-call: check 0x, 7, 3 => 261

// ABI padding must preserve the heap bound used to spill values across the copy.
contract AbiPaddingBoundedCopy {
    // Keep enough values live across the final copy to require the heap proof.
    function check(bytes calldata data, uint256 seed, uint256 count) external pure returns (uint256) {
        uint256 ptr = allocate(count * 32);
        bytes32 hash;
        assembly {
            let offset := 0
            for { let i := 0 } lt(i, count) { i := add(i, 1) } {
                mstore(add(ptr, offset), i)
                offset := add(offset, 32)
            }
            hash := keccak256(ptr, mul(count, 32))
        }
        hash ^= keccak256(abi.encodePacked(seed, data, uint128(count)));
        bytes memory encoded = abi.encode(data, count);
        ptr = allocate(data.length);
        uint256 a = seed + 1;
        uint256 b = seed + 2;
        uint256 c = seed + 3;
        uint256 d = seed + 4;
        uint256 e = seed + 5;
        uint256 f = seed + 6;
        uint256 g = seed + 7;
        uint256 h = seed + 8;
        uint256 i = seed + 9;
        uint256 j = seed + 10;
        uint256 k = seed + 11;
        uint256 l = seed + 12;
        assembly { calldatacopy(ptr, data.offset, data.length) }
        if (data.length > 0) {
            assembly { if iszero(eq(byte(0, mload(ptr)), byte(0, calldataload(data.offset)))) { revert(0, 0) } }
        }
        return a + b + c + d + e + f + g + h + i + j + k + l + count + encoded.length + (hash == 0 ? 1 : 0);
    }

    function allocate(uint256 size) internal pure returns (uint256 ptr) {
        assembly {
            ptr := mload(0x40)
            let end := add(ptr, and(add(size, 31), not(31)))
            if or(gt(end, 0xffffffffffffffff), lt(end, ptr)) { revert(0, 0) }
            mstore(0x40, end)
        }
    }
}
