// solc divides literals exactly, which puts these base slots at 2 and 7. The compiler does not
// support fractional values yet, so it rejects them instead of truncating to another slot.

contract WideFraction layout at 2**300 / 3 * 3 / 2**299 { uint x; } //~ ERROR: cannot apply builtin operator `/`
contract Fraction layout at 7 / 3 * 3 { uint x; } //~ ERROR: base slot of storage layout must evaluate to an integer
contract WideExact layout at 2**300 / 2**299 { uint x; }
contract Exact layout at 12 / 3 * 3 { uint x; }
