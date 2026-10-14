// solc rejects conditionals in base slots with error 1505; see CONTRACT-001.

uint constant T = true ? 1 : 0;

contract A layout at true ? 42 : 94 {}
// The conditional has type `uint8`, like the literals in its branches, so the sum overflows.
contract B layout at 255 + (true ? 1 : 0) {} //~ ERROR: failed to evaluate constant: arithmetic overflow
contract C layout at T {}
