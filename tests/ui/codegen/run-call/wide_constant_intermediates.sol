//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: uint256Max() => true
//@ run-call: wideIntermediate() => true
//@ run-call: cancelledPower() => true

contract WideConstantIntermediates {
    function uint256Max() external pure returns (bool) {
        return 2**256 - 1 == type(uint256).max;
    }

    function wideIntermediate() external pure returns (bool) {
        return (2**256 + 1) * 2 - 2**256 - 3 == type(uint256).max;
    }

    function cancelledPower() external pure returns (bool) {
        return (2**512 - 1) - (2**512 - 2**256) == type(uint256).max;
    }
}
