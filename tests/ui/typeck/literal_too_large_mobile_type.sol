// A literal too large for any integer type has no mobile type, so solc rejects it where the
// literal would need one: as an ABI-encoded argument, a statement or a tuple component.

contract C {
    function encode() public pure {
        abi.encode(2**256); //~ ERROR: literal is too large for any integer type
        abi.encode(115792089237316195423570985008687907853269984665640564039457584007913129639935 + 1); //~ ERROR: literal is too large for any integer type
        abi.encodeWithSelector(0x12345678, 2**255 * 4); //~ ERROR: literal is too large for any integer type
        abi.encodeWithSignature("f(int256)", -(2**255) - 1); //~ ERROR: literal is too large for any integer type
        abi.encodePacked(2**256); //~ ERROR: literal is too large for any integer type
        abi.encode(2**256 - 1, -(2**255));
    }

    function statement() public pure {
        2**256; //~ ERROR: literal is too large for any integer type
        (2**256); //~ ERROR: literal is too large for any integer type
        2**256 - 1;
    }

    function tuple() public pure {
        (2**256, 1); //~ ERROR: literal is too large for any integer type
        (2**256 - 1, 1);
    }
}
