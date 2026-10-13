// solc rejects conditionals in base slots with error 1505; see CONTRACT-001.

uint constant T = true ? 1 : 0;

contract A layout at true ? 42 : 94 {}
// Literal ternaries use their common mobile integer type, so this sum overflows `uint8`.
contract B layout at 255 + (true ? 1 : 0) {} //~ ERROR: failed to evaluate constant: arithmetic overflow
contract Wide layout at 255 + (true ? T : 0) {}
contract C layout at T {}
