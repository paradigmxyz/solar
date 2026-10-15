// An error in a base slot does not hide the cycles of other constants. solc reports only the cycle.
uint8 constant ONE = 1;
uint constant Z = Z + 1; //~ ERROR: the value of the constant `Z` has a cyclic dependency via `Z`

contract X layout at ONE << 8 {} //~ ERROR: failed to evaluate constant: arithmetic overflow
