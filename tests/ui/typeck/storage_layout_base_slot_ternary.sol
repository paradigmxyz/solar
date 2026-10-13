// solc rejects conditionals in base slots with error 1505; see CONTRACT-001.

uint constant T = true ? 1 : 0;

contract A layout at true ? 42 : 94 {}
contract B layout at 255 + (true ? 1 : 0) {}
contract C layout at T {}
