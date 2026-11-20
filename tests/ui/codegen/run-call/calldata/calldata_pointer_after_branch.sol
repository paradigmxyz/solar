//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: f 7 => 7
//@ run-call-fail: f 0

// An assembly assignment to an uninitialized static calldata array keeps the length of its type.
contract CalldataPointerAfterBranch {
    function f(uint256 x) external pure returns (uint256) {
        uint256[2] calldata values;
        if (x == 0) {
            revert();
        }
        assembly {
            values := 4
        }
        return values[0];
    }
}
