bytes32 constant L = 0x0000000000000000000000000000000000000000000000000000000000000002;
bytes1 constant B = 0x02;

contract Constant layout at L {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract Shifted layout at B >> 1 {} //~ ERROR: base slot of storage layout must evaluate to an integer
