// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/173_constant_struct.sol
contract C {
    struct S { uint x; uint[] y; }
    S constant x = S(5, new uint[](4)); //~ ERROR: only constants of value type and byte array type are implemented
}
