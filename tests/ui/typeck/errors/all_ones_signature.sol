// ported-from: test/libsolidity/syntaxTests/errors/all_ones_signature.sol
error test266151307(); //~ ERROR: the selector `0xffffffff` is reserved
contract C {
    error test266151307(); //~ ERROR: the selector `0xffffffff` is reserved
}
