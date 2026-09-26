//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: pair 1, 2 => 0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0
//@ run-call: mixed -1, 0xdeadbeef => 0x85938ad1bef66a6ac49574c232aa1eea22143af97a7ffb43428c03f4acaa01b1
//@ run-call: single true => 0xb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6

// Hashing one or two words. `keccak256(abi.encode(a, b))` needs no library:
// the encoding is two words, so it is built in scratch space and hashed
// there, with no allocation and no free-pointer traffic.
// CHECK-LABEL: fn @pair
// CHECK: mstore 0, arg0
// CHECK: mstore 32, arg1
// CHECK: keccak256 0, 64
contract Safe {
    function pair(uint256 a, uint256 b) public pure returns (bytes32) {
        return keccak256(abi.encode(a, b));
    }

    function mixed(int8 a, bytes4 b) public pure returns (bytes32) {
        return keccak256(abi.encode(a, b));
    }

    function single(bool a) public pure returns (bytes32) {
        return keccak256(abi.encode(a));
    }
}
