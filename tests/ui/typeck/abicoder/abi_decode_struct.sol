// ported-from: test/libsolidity/syntaxTests/specialFunctions/abidecode/abi_decode_struct.sol
pragma abicoder v1;
struct S {
    uint x;
}

contract C {
    function f() public pure {
        abi.decode("1234", (S)); //~ ERROR: decoding type not supported
    }
}
