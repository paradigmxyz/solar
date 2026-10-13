//@ compile-flags: --emit=abi,hashes --pretty-json

// Bare `fixed` and `ufixed` are `fixed128x18` and `ufixed128x18`.
library L {
    function f(fixed x) external pure returns (uint) { return 1; }
    function f(fixed256x0 x) external pure returns (uint) { return 2; }
    function g(ufixed x, ufixed8x80 y) external pure {}
}
