//@ compile-flags: -Zdump=mir

// `address(L)` needs the library's deploy-time address, which lowering cannot
// supply yet. The bail-out is reported rather than left as an `INVALID` body,
// so the compilation fails instead of producing code that traps at runtime.
library L {
    function f(uint256 v) external pure returns (uint256) {
        return v;
    }
}

contract C {
    function addr() public pure returns (address) { //~ ERROR: codegen rewrite does not support this function yet
        return address(L);
    }

    // A library function's selector needs no address, and still lowers: a
    // silent bail-out here would be reported as a second error, and
    // `library_selector.sol` pins the MIR it lowers to.
    function sel() public pure returns (bytes4) {
        return L.f.selector;
    }
}
