//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: twice => 1
//@ run-call: thrice => 3
//@ run-call: outerReturns => 0
//@ run-call: threeSame => 11
//@ run-call: returnsSet => 7, 1001

// An early `return` ends only the modifier invocation that executes it. The enclosing invocation
// of the same modifier continues with its own parameter.
contract NestedSameReturn {
    uint256 x;

    modifier m(uint256 k) {
        if (k == 2) return;
        _;
        x = x * 10 + k;
    }

    modifier after_(uint256 k) {
        _;
        if (k == 2) return;
        x = x * 10 + k;
    }

    function twice() external returns (uint256) {
        twice1();
        return x;
    }

    function thrice() external returns (uint256) {
        thrice1();
        return x;
    }

    function outerReturns() external returns (uint256) {
        outerReturns1();
        return x;
    }

    function threeSame() external returns (uint256) {
        threeSame1();
        return x;
    }

    function returnsSet() external returns (uint256, uint256) {
        uint256 r = returnsSet1();
        return (r, x);
    }

    function twice1() internal m(1) m(2) {
        x = 100;
    }

    function thrice1() internal m(3) m(2) m(4) {
        x = 100;
    }

    function outerReturns1() internal m(2) m(1) {
        x = 100;
    }

    function threeSame1() internal m(1) m(1) m(2) {
        x = 100;
    }

    function returnsSet1() internal after_(1) after_(2) returns (uint256 r) {
        r = 7;
        x = 100;
    }
}
