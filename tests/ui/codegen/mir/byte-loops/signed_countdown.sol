//@ compile-flags: -Ogas
//@ run-call: decrementFirst 0x0102, 2 => 258
//@ run-call: decrementFirst 0x0102, 0 => 0
//@ run-call-fail: decrementFirst 0x0102, -57896044618658097711785492504343953926634992332820282019728792003956564819968 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: decrementLast 0x0102, 2 => 258
//@ run-call-fail: decrementLast 0x0102, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: decrementLast 0x0102, -57896044618658097711785492504343953926634992332820282019728792003956564819968 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: decrementThenIndex 0x0102, 2 => 258
//@ run-call-fail: decrementThenIndex 0x0102, -57896044618658097711785492504343953926634992332820282019728792003956564819968 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// A signed `k != 0` countdown keeps the panics of its checked decrement and index checks, whether
// the decrement runs before or after the accesses.
contract Test {
    function decrementFirst(bytes calldata src, int256 k) external pure returns (uint256) {
        bytes memory out = new bytes(32);
        while (k != 0) {
            --k;
            out[uint256(k)] = src[uint256(k)];
        }
        return uint256(uint8(out[0])) * 256 + uint8(out[1]);
    }

    function decrementLast(bytes calldata src, int256 k) external pure returns (uint256) {
        bytes memory out = new bytes(32);
        while (k != 0) {
            out[uint256(k) - 1] = src[uint256(k) - 1];
            --k;
        }
        return uint256(uint8(out[0])) * 256 + uint8(out[1]);
    }

    function decrementThenIndex(bytes calldata src, int256 k) external pure returns (uint256) {
        bytes memory out = new bytes(32);
        while (k != 0) {
            int256 j = k;
            --k;
            out[uint256(j) - 1] = src[uint256(j) - 1];
        }
        return uint256(uint8(out[0])) * 256 + uint8(out[1]);
    }
}
