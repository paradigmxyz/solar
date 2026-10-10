// ported-from: test/libsolidity/syntaxTests/constants/initialization/abi_encoding_builtin_non_const_args.sol
contract C {
    uint k = 1;

    bytes32 constant a = keccak256(abi.encode(1, k)); //~ ERROR: initial value for constant variable has to be compile-time constant
    bytes32 constant b = keccak256(abi.encodePacked(uint(1), k)); //~ ERROR: initial value for constant variable has to be compile-time constant
    bytes32 constant c = keccak256(abi.encodeWithSelector(0x12345678, k, 2)); //~ ERROR: initial value for constant variable has to be compile-time constant
    bytes32 constant d = keccak256(abi.encodeWithSignature("f()", 1, k)); //~ ERROR: initial value for constant variable has to be compile-time constant
}
