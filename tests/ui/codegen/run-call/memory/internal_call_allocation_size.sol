//@ revisions: none gas size
//@ compile-flags: --no-warnings
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: f => 64
contract C {
    struct S {
        uint256 a;
        uint256 b;
    }

    function inner() internal pure returns (uint256 delta) {
        uint256 before;
        assembly {
            before := mload(0x40)
        }
        S memory s;
        assembly {
            delta := sub(mload(0x40), before)
        }
    }

    // solc: 64, the two words of `s`. Inlining the body into `f` by hand also gives 64 here.
    function f() external pure returns (uint256) {
        return inner();
    }
}
