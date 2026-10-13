// solc evaluates this base slot exactly to 7; see TYPECK-017.

contract Fraction layout at 7 / 3 * 3 { uint x; } //~ ERROR: base slot of storage layout must evaluate to an integer
contract Exact layout at 12 / 3 * 3 { uint x; }
