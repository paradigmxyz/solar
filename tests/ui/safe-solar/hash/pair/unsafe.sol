//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: pair 1, 2 => 0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0
//@ run-call: mixed -1, 0xdeadbeef => 0x85938ad1bef66a6ac49574c232aa1eea22143af97a7ffb43428c03f4acaa01b1
//@ run-call: single true => 0xb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6

// The same digests from assembly, as the hashing libraries write them. The
// encoding is the caller's to get right: the sign extension of `a` and the
// left alignment of `b` are by hand here.
// CHECK-LABEL: fn @pair
// CHECK: keccak256 0, 64
contract Unsafe {
    function pair(uint256 a, uint256 b) public pure returns (bytes32 h) {
        assembly ("memory-safe") {
            mstore(0x00, a)
            mstore(0x20, b)
            h := keccak256(0x00, 0x40)
        }
    }

    function mixed(int8 a, bytes4 b) public pure returns (bytes32 h) {
        assembly ("memory-safe") {
            mstore(0x00, signextend(0, a))
            mstore(0x20, b)
            h := keccak256(0x00, 0x40)
        }
    }

    function single(bool a) public pure returns (bytes32 h) {
        assembly ("memory-safe") {
            mstore(0x00, a)
            h := keccak256(0x00, 0x20)
        }
    }
}
