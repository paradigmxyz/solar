// ported-from: test/libsolidity/syntaxTests/constants/initialization/math_builtin_opcode_based_non_const_args.sol
contract A {
    uint256 k = 7;
    uint256 constant amod = addmod(1, 8, k); //~ ERROR: initial value for constant variable has to be compile-time constant
    uint256 constant mmod = mulmod(1, 8, k); //~ ERROR: initial value for constant variable has to be compile-time constant

    bytes data = hex"ffff";
    bytes32 constant keccak = keccak256(data); //~ ERROR: initial value for constant variable has to be compile-time constant
}
