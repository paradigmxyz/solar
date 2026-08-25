//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@[none, gas, size] run-call-fail: C::unusedReturn() => 0x

interface Target {
    function value() external view returns (uint256);
}

contract C {
    // Even without a return binding, successful returndata must be decoded.
    // Calling an address without code therefore reverts outside the catch.
    function unusedReturn() external view returns (uint256) {
        try Target(0x1111111111111111111111111111111111111111).value() {
            return 1;
        } catch {
            return 2;
        }
    }
}
