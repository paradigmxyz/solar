//@compile-flags: -Ztypeck
contract C {

    function f(bytes memory a1) public pure
    {

        // Valid conversion
        bytes1 b1 = bytes1(a1);
        bytes2 b2 = bytes2(a1);
        bytes10 b3 = bytes10(a1);
        bytes16 b4 = bytes16(a1);

        // Invalid Dynamic bytes conversion
        bytes memory a1 = bytes(b4); //~ERROR: cannot convert `bytes16` to `bytes`
        bytes memory a2 = bytes(b3); //~ERROR: cannot convert `bytes10` to `bytes`
        bytes memory a3 = bytes(b2); //~ERROR: cannot convert `bytes2` to `bytes`
        bytes memory a4 = bytes(b1); //~ERROR: cannot convert `bytes1` to `bytes`

    }
}
