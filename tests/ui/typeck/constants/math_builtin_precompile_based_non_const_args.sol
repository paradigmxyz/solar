// ported-from: test/libsolidity/syntaxTests/constants/initialization/math_builtin_precompile_based_non_const_args.sol
contract A {
    bytes data = hex"ffff";
    bytes32 constant sha = sha256(data); //~ ERROR: initial value for constant variable has to be compile-time constant
    bytes20 constant ripemd = ripemd160(data); //~ ERROR: initial value for constant variable has to be compile-time constant
    address constant addr = ecrecover("1234", 1, "0", abi.decode(data, (bytes2))); //~ ERROR: initial value for constant variable has to be compile-time constant
}
