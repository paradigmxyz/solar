//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: createPair => 17, 100, 200, 4660
//@ run-call: createSingle => 5, 1, 2, 153
//@ run-call: check 0x112233, 7, 3 => 293
//@ run-call: check 0x, 7, 3 => 261

// A variable-length copy into low memory requires spilled values live
// across it to remain on the stack. The creation payload's free-memory-pointer load
// comes after the copy in the same block; its slot is reserved before its
// definition stores it, so the copy must not reload it as a live value.
// NOTE: Unoptimized builds keep internal calls' callers off the resident
// stack layout, so they still reject values live across such a copy here.
contract Child {
    uint256 public start;
    uint256 public b0;
    uint256 public b1;
    address public r0;

    constructor(address[] memory receivers, uint256[] memory batches, uint256 startingId) {
        start = startingId;
        b0 = batches[0];
        b1 = batches[1];
        r0 = receivers[0];
    }
}

contract ForwardingBufferLaterFmp {
    function one(address account) internal pure returns (address[] memory accounts) {
        accounts = new address[](1);
        accounts[0] = account;
    }

    function createPair() external returns (uint256, uint256, uint256, uint256) {
        address[] memory receivers = new address[](2);
        receivers[0] = address(0x1234);
        receivers[1] = address(0x5678);
        uint256[] memory batches = new uint256[](2);
        batches[0] = 100;
        batches[1] = 200;
        assembly {
            returndatacopy(0, 0, returndatasize())
        }
        Child token = new Child(receivers, batches, 17);
        return (token.start(), token.b0(), token.b1(), uint160(token.r0()));
    }

    function createSingle() external returns (uint256, uint256, uint256, uint256) {
        uint256[] memory batches = new uint256[](2);
        batches[0] = 1;
        batches[1] = 2;
        assembly {
            returndatacopy(0, 0, returndatasize())
        }
        Child token = new Child(one(address(0x99)), batches, 5);
        return (token.start(), token.b0(), token.b1(), uint160(token.r0()));
    }

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
