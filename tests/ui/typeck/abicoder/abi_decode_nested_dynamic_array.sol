// ported-from: test/libsolidity/syntaxTests/specialFunctions/abidecode/abi_decode_nested_dynamic_array.sol
pragma abicoder v1;
contract C {
    function f() public pure {
        abi.decode("1234", (uint[][3])); //~ ERROR: decoding type not supported
    }
}
