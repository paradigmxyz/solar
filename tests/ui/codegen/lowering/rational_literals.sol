//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: fee() => 5000000000000000
//@ run-call: directFee() => 5000000000000000
//@ run-call: signedTyped() => -6
//@ run-call: tiny() => 1
//@ run-call: seven() => 7
//@ run-call: decimal() => 5
//@ run-call: negative() => -7
//@ run-call: remainder() => 1
//@ run-call: inverse() => 4
//@ run-call: wide() => 57896044618658097711785492504343953926634992332820282019728792003956564819968
//@ run-call: max() => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: typed() => 6
//@ run-call: runtimeDivision 7 => 6
//@ run-call: compare() => true

contract RationalLiterals {
    uint constant SEVEN = 7;
    uint constant TWO = 2;
    int constant NEGATIVE_SEVEN = -7;
    int constant SIGNED_TWO = 2;
    uint constant FEE = (5 / 1000) * 1e18;

    // CHECK-LABEL: fn @fee(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 0x11c37937e08000
    function fee() public pure returns (uint) { return FEE; }
    // CHECK-LABEL: fn @directFee(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 0x11c37937e08000
    function directFee() public pure returns (uint) { return (5 / 1000) * 1e18; }
    // CHECK-LABEL: fn @signedTyped(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: [[SD:v[0-9]+]] = checked_div i256, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff9, 2
    // CHECK-NEXT: [[SM:v[0-9]+]] = checked_mul i256, [[SD]], 2
    // CHECK-NEXT: ret [[SM]]
    function signedTyped() public pure returns (int) { return (NEGATIVE_SEVEN / SIGNED_TWO) * SIGNED_TWO; }
    // CHECK-LABEL: fn @tiny(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 1
    function tiny() public pure returns (uint) { return (1 / (2 ** 300)) * (2 ** 300); }
    // CHECK-LABEL: fn @seven(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 7
    function seven() public pure returns (uint) { return (7 / 2) * 2; }
    // CHECK-LABEL: fn @decimal(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 5
    function decimal() public pure returns (uint) { return 0.5 * 10; }
    // CHECK-LABEL: fn @negative(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff9
    function negative() public pure returns (int) { return (-7 / 2) * 2; }
    // CHECK-LABEL: fn @remainder(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 1
    function remainder() public pure returns (uint) { return ((7 / 2) % (3 / 2)) * 2; }
    // CHECK-LABEL: fn @inverse(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 4
    function inverse() public pure returns (uint) { return (1 / 2) ** -2; }
    // CHECK-LABEL: fn @wide(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 0x8000000000000000000000000000000000000000000000000000000000000000
    function wide() public pure returns (uint) { return (1 << 300) >> 45; }
    // CHECK-LABEL: fn @max(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
    function max() public pure returns (uint) { unchecked { return 2**256 - 1; } }
    // CHECK-LABEL: fn @typed(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: [[UD:v[0-9]+]] = checked_div u256, 7, 2
    // CHECK-NEXT: [[UM:v[0-9]+]] = checked_mul u256, [[UD]], 2
    // CHECK-NEXT: ret [[UM]]
    function typed() public pure returns (uint) { return (SEVEN / TWO) * TWO; }
    // CHECK-LABEL: fn @runtimeDivision(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: [[RD:v[0-9]+]] = checked_div u256, arg0, 2
    // CHECK-NEXT: [[RM:v[0-9]+]] = checked_mul u256, [[RD]], 2
    // CHECK-NEXT: ret [[RM]]
    function runtimeDivision(uint x) public pure returns (uint) { return (x / 2) * 2; }
    // CHECK-LABEL: fn @compare(
    // CHECK: eq 7, 7
    // CHECK: eq 0x11c37937e08000, 0x11c37937e08000
    function compare() public pure returns (bool) { return (7 / 2) * 2 == 7 && (5 / 1000) * 1e18 == 5e15; }
}
