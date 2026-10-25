//@ codegen-matrix: standard
//@ compile-flags: --evm-version byzantium
//@ run-call: shl 1, 0 => 1
//@ run-call: shl 1, 255 => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: shl 3, 255 => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: shl 1, 256 => 0
//@ run-call: shl 0x8000000000000000000000000000000000000000000000000000000000000000, 1 => 0
//@ run-call: shr 0x8000000000000000000000000000000000000000000000000000000000000000, 255 => 1
//@ run-call: shr 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 256 => 0
//@ run-call: shr 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: sar -1, 0 => -1
//@ run-call: sar -1, 255 => -1
//@ run-call: sar -1, 256 => -1
//@ run-call: sar -8, 1 => -4
//@ run-call: sar -57896044618658097711785492504343953926634992332820282019728792003956564819968, 255 => -1
//@ run-call: sar 57896044618658097711785492504343953926634992332820282019728792003956564819967, 254 => 1
//@ run-call: sar 57896044618658097711785492504343953926634992332820282019728792003956564819967, 255 => 0
//@ run-call: sar 5, 256 => 0
//@ run-call: sar -5, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => -1

// Before Constantinople, variable and arithmetic shifts become `exp`-based arithmetic. The
// boundaries are the shift amounts 0, 255 and 256 or more, and the sign and top bit of the value.
contract LegacyVariableShifts {
    function shl(uint256 x, uint256 s) external pure returns (uint256) {
        return x << s;
    }

    function shr(uint256 x, uint256 s) external pure returns (uint256) {
        return x >> s;
    }

    function sar(int256 x, uint256 s) external pure returns (int256) {
        return x >> s;
    }
}
