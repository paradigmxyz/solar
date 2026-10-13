// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/171_assignment_to_const_array_vars.sol
contract C {
    uint[3] constant x = [uint(1), 2, 3]; //~ ERROR: only constants of value type and byte array type are implemented
}
