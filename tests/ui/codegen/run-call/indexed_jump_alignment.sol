//@ revisions: gas low size legacy
//@ compile-flags: -Zswitch-lowering=buckets
//@[gas] compile-flags: -Ogas --optimize-runs=1000000 --evm-version=cancun
//@[low] compile-flags: -Ogas --optimize-runs=200 --evm-version=cancun
//@[size] compile-flags: -Osize --optimize-runs=1000000 --evm-version=cancun
//@[legacy] compile-flags: -Ogas --optimize-runs=1000000 --evm-version=byzantium
//@ run-call: f0 => 10
//@ run-call: f1 => 11
//@ run-call: f2 => 12
//@ run-call: f3 => 13
//@ run-call: f4 => 14
//@ run-call: f5 => 15
//@ run-call: f6 => 16
//@ run-call: f7 => 17
//@ run-call: f8 => 18
//@ run-call: f9 => 19
//@ run-call: f10 => 20
//@ run-call: f11 => 21
//@ run-call: f12 => 22
//@ run-call: f13 => 23
//@ run-call: f14 => 24
//@ run-call: f15 => 25
//@ run-call-fail: 0xffffffff => 0x
//@ run-call-fail: 0x => 0x

contract ByteTableDispatch {
    function f0() external pure returns (uint256) { return 10; }
    function f1() external pure returns (uint256) { return 11; }
    function f2() external pure returns (uint256) { return 12; }
    function f3() external pure returns (uint256) { return 13; }
    function f4() external pure returns (uint256) { return 14; }
    function f5() external pure returns (uint256) { return 15; }
    function f6() external pure returns (uint256) { return 16; }
    function f7() external pure returns (uint256) { return 17; }
    function f8() external pure returns (uint256) { return 18; }
    function f9() external pure returns (uint256) { return 19; }
    function f10() external pure returns (uint256) { return 20; }
    function f11() external pure returns (uint256) { return 21; }
    function f12() external pure returns (uint256) { return 22; }
    function f13() external pure returns (uint256) { return 23; }
    function f14() external pure returns (uint256) { return 24; }
    function f15() external pure returns (uint256) { return 25; }
}
