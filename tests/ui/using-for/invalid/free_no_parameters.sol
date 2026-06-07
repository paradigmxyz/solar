//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/using_free_no_parameters_err.sol

function zero() pure returns (uint256) {
    return 0;
}

contract C {
    using {zero} for uint256; //~ ERROR: does not have any parameters
}
