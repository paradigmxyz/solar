// ported-from: test/libsolidity/syntaxTests/specialFunctions/abi_encode_nested_dynamic_array.sol
pragma abicoder v1;
contract C {
    function test() public pure {
        abi.encode([new uint[](5), new uint[](7)]); //~ ERROR: `encode` argument cannot be ABI-encoded
    }
}
