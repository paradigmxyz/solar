// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/408_invalid_address_length_short.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/409_invalid_address_length_long.sol

// A hexadecimal literal with 39 or 41 digits looks like an address, but is not one. With 38 or 42
// digits it is an integer, and with 40 it is an address.
contract C {
    uint256 i38 = 0x11111111111111111111111111111111111111;
    uint256 i42 = 0x111111111111111111111111111111111111111111;
    address a40 = 0x1111111111111111111111111111111111111111;

    address a39 = 0x111111111111111111111111111111111111111; //~ ERROR: this looks like an address but has 39 hex digits instead of 40
    address a41 = 0x11111111111111111111111111111111111111111; //~ ERROR: this looks like an address but has 41 hex digits instead of 40

    // The error is the same when the literal is used as an integer.
    uint256 i39 = 0x111111111111111111111111111111111111111; //~ ERROR: this looks like an address but has 39 hex digits instead of 40
    //~^ ERROR: mismatched types
    uint256 i41 = 0x11111111111111111111111111111111111111111; //~ ERROR: this looks like an address but has 41 hex digits instead of 40
    //~^ ERROR: mismatched types

    // Underscores are not digits.
    address u39 = 0x1111_1111_1111_1111_1111_1111_1111_1111_1111_111; //~ ERROR: this looks like an address but has 39 hex digits instead of 40
    address u40 = 0x1111_1111_1111_1111_1111_1111_1111_1111_1111_1111;
    address u41 = 0x1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1; //~ ERROR: this looks like an address but has 41 hex digits instead of 40

    // Prepending `00` to an address makes an integer.
    uint256 z42 = 0x001111111111111111111111111111111111111111;

    // Yul literals are plain numbers, so any length is fine.
    function f() public pure returns (uint256 r) {
        assembly {
            r := 0x111111111111111111111111111111111111111
            r := 0x11111111111111111111111111111111111111111
        }
    }
}
