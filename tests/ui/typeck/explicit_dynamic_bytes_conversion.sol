//@compile-flags: -Ztypeck
contract C {
    function f(bytes memory a0) public pure {
        // Valid conversion
        bytes1 b1 = bytes1(a0);
        bytes2 b2 = bytes2(a0);
        bytes10 b3 = bytes10(a0);
        bytes16 b4 = bytes16(a0);

        // Invalid Dynamic bytes conversion
        bytes memory a1 = bytes(b4); //~ERROR: invalid explicit type conversion
        bytes memory a2 = bytes(b3); //~ERROR: invalid explicit type conversion
        bytes memory a3 = bytes(b2); //~ERROR: invalid explicit type conversion
        bytes memory a4 = bytes(b1); //~ERROR: invalid explicit type conversion
    }
}
