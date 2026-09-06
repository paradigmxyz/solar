//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call: ProtocolWriter15::run 0, 1, 0 => 91
//@ run-call: ProtocolWriter15::run 0, 1, 32 => 91
//@ run-call-fail: ProtocolWriter15::run 0, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff; gas=1000000
//@ run-call: ProtocolWriter15::run 1, 1, 0 => 91
//@ run-call: ProtocolWriter15::run 1, 1, 32 => 91
//@ run-call-fail: ProtocolWriter15::run 1, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff; gas=1000000
//@ run-call: ProtocolWriter15::run 2, 1, 0 => 91
//@ run-call: ProtocolWriter15::run 2, 1, 32 => 91
//@ run-call-fail: ProtocolWriter15::run 2, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff; gas=1000000
//@ run-call: ProtocolWriter13::run 0, 1, 0 => 66
//@ run-call: ProtocolWriter13::run 0, 1, 32 => 66
//@ run-call-fail: ProtocolWriter13::run 0, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff; gas=1000000
//@ run-call: ProtocolWriter13::run 1, 1, 0 => 66
//@ run-call: ProtocolWriter13::run 1, 1, 32 => 66
//@ run-call-fail: ProtocolWriter13::run 1, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff; gas=1000000
//@ run-call: ProtocolWriter13::run 2, 1, 0 => 66
//@ run-call: ProtocolWriter13::run 2, 1, 32 => 66
//@ run-call-fail: ProtocolWriter13::run 2, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff; gas=1000000

// Protocol backups must participate in stack-pressure planning. The 15-argument
// body previously failed to compile; the 13-argument body already fit DUP16
// with two relative header backups. All calls name their contract explicitly.
contract ProtocolWriter15 {
    // CHECK-LABEL: @module ProtocolWriter15
    function run(uint256 depth, uint256 x, uint256 n) external pure returns (uint256) {
        return recurse(depth, n, x, x + 1, x + 2, x + 3, x + 4, x + 5, x + 6, x + 7, x + 8, x + 9, x + 10, x + 11, x + 12);
    }

    // CHECK-LABEL: fn @recurse
    // CHECK: calldatacopy 0x10000, 0, arg1
    function recurse(uint256 depth, uint256 n, uint256 a0, uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11, uint256 a12)
        internal pure returns (uint256)
    {
        assembly { calldatacopy(65536, 0, n) }
        if (depth != 0) return recurse(depth - 1, n, a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12);
        unchecked { return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11 + a12; }
    }
}

contract ProtocolWriter13 {
    // CHECK-LABEL: @module ProtocolWriter13
    function run(uint256 depth, uint256 x, uint256 n) external pure returns (uint256) {
        return recurse(depth, n, x, x + 1, x + 2, x + 3, x + 4, x + 5, x + 6, x + 7, x + 8, x + 9, x + 10);
    }

    // CHECK-LABEL: fn @recurse
    // CHECK: calldatacopy 0x10000, 0, arg1
    function recurse(uint256 depth, uint256 n, uint256 a0, uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10)
        internal pure returns (uint256)
    {
        assembly { calldatacopy(65536, 0, n) }
        if (depth != 0) return recurse(depth - 1, n, a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10);
        unchecked { return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10; }
    }
}
