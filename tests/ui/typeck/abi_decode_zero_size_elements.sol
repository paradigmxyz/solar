// Arrays of elements that encode to zero bytes cannot be decoded; solc crashes on them.

contract C {
    function f(bytes memory d) public pure {
        abi.decode(d, (uint[0][])); //~ ERROR: decoding type not supported
        abi.decode(d, (uint[0][2])); //~ ERROR: decoding type not supported
        abi.decode(d, (uint[2][0][])); //~ ERROR: decoding type not supported
        abi.decode(d, (uint[0][2][3])); //~ ERROR: decoding type not supported
        abi.decode(d, (C[0][])); //~ ERROR: decoding type not supported

        abi.decode(d, (uint[][0][], bytes[0][2]));
    }
}
