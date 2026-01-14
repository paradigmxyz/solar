// Valid constant declarations at file and contract level

uint constant FILE_CONST = 42;
int constant NEGATIVE = -1;
bool constant FLAG = true;
bytes32 constant HASH = keccak256("test");

contract C {
    uint constant CONTRACT_CONST = 100;
    int constant public transient = 0; // transient is valid identifier
}
