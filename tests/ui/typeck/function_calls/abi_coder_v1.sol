//@compile-flags: -Ztypeck

pragma abicoder v1;

contract C {
    struct S {
        uint256 x;
    }

    S s;
    uint256[][3] arrays;

    function test(bytes memory data) public view {
        abi.encode(s); //~ ERROR: type cannot be encoded
        abi.encode(arrays); //~ ERROR: type cannot be encoded

        abi.decode(data, (S)); //~ ERROR: decoding type not supported
        abi.decode(data, (uint256[][3])); //~ ERROR: decoding type not supported
    }
}
