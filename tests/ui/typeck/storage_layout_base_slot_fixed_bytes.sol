bytes32 constant L = 0x0000000000000000000000000000000000000000000000000000000000000002;
bytes1 constant B = 0x02;

contract Constant layout at L {} //~ ERROR: `bytesN` constants are not integers
contract Shifted layout at B >> 1 {} //~ ERROR: `bytesN` constants are not integers
