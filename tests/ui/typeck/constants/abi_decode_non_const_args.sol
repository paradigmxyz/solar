// ported-from: test/libsolidity/syntaxTests/constants/initialization/abi_decode_non_const_args.sol
contract A {
    function encoded() private view returns (bytes memory) {
        return abi.encode(hex"aaaa");
    }

    bytes constant a = abi.decode(encoded(), (bytes)); //~ ERROR: initial value for constant variable has to be compile-time constant
}
