//@ codegen-matrix: standard
//@ compile-flags: --libraries L=0x1111111111111111111111111111111111111111
//@ run-call: C::addr => 0x1111111111111111111111111111111111111111
//@ run-call: C::plusOne => 0x1111111111111111111111111111111111111112

// A library name in inline assembly is the library's address.

library L {
    function f(uint256 v) external pure returns (uint256) {
        return v;
    }
}

contract C {
    function addr() public pure returns (address a) {
        assembly {
            a := L
        }
    }

    function plusOne() public pure returns (uint256 r) {
        assembly {
            let x := L
            r := add(x, 1)
        }
    }
}
