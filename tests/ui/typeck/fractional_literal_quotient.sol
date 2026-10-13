// solc keeps literal quotients exact; see TYPECK-017. Typed arithmetic would truncate them, so the
// compiler rejects them wherever they appear.

contract C {
    uint constant HALF = 7 / 2; //~ ERROR: fractional literal quotients are not supported

    function scaled() public pure returns (uint) {
        return 7 / 2 * 100; //~ ERROR: fractional literal quotients are not supported
    }

    function negative() public pure returns (int) {
        return -(7 / 2) * 2; //~ ERROR: fractional literal quotients are not supported
    }

    function exact() public pure returns (uint) {
        return 8 / 2 * 100;
    }
}
