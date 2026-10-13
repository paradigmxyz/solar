// Bare `fixed` is `fixed128x18`, so these overloads have the same parameter types.
library L {
    function f(fixed x) external pure returns (uint) { return 1; } //~ ERROR: function with same name and parameter types declared twice
    function f(fixed128x18 x) external pure returns (uint) { return 2; } //~ NOTE: other declaration
}
