// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/585_abi_decode_with_unsupported_types.sol
pragma abicoder v1;
contract C {
    struct s { uint a; uint b; }
    function f() pure public {
        abi.decode("", (s)); //~ ERROR: decoding type not supported
    }
}
