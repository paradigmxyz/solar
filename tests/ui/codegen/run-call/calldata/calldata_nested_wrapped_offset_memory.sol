//@ codegen-matrix: standard
//@ run-call-fail: 0xc85cd17800000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5cffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5c => 0x
//@ run-call-fail: 0x7efc0ed300000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5cffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5c => 0x
//@ run-call-fail: 0xd4df0d7d00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5cffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5c => 0x
//@ run-call-fail: 0xdf1b827100000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5c => 0x
//@ run-call-fail: 0xeda7d6660000000000000000000000000000000000000000000000000000000000000020ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5cffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5c => 0x
//@ run-call-fail: 0xdf1b827100000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff9c => 0x
//@ run-call-fail: 0xeda7d6660000000000000000000000000000000000000000000000000000000000000020ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff9cffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff9c => 0x

// A nested ABI offset that is negative wraps the pointer of an array or struct above calldata. The
// copy to memory checks the address as an unsigned value, so it reverts, as solc does, instead of
// reading zero-filled calldata.
contract C {
    struct S {
        uint256 x;
        uint256[] v;
    }

    function arr(uint256[][] calldata a) external pure returns (uint256[] memory) {
        return a[0];
    }

    function sarr(S[] calldata a) external pure returns (uint256[] memory) {
        return a[0].v;
    }

    function copyS(S[] calldata a) external pure returns (S memory) {
        return a[0];
    }

    function st(S calldata s) external pure returns (uint256[] memory) {
        return s.v;
    }

    function fxu(uint256[][2] calldata a) external pure returns (uint256[] memory) {
        return a[0];
    }
}
