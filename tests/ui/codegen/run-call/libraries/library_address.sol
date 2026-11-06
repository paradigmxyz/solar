//@ codegen-matrix: standard
//@ compile-flags: --libraries L=0x1111111111111111111111111111111111111111
//@ run-call: C::addr => 0x1111111111111111111111111111111111111111

library L {
    function f(uint256 v) external pure returns (uint256) {
        return v;
    }
}

contract C {
    function addr() public pure returns (address) {
        return address(L);
    }

    function sel() public pure returns (bytes4) {
        return L.f.selector;
    }
}
