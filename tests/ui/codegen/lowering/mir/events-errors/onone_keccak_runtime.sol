//@ revisions: none gas
//@[none] compile-flags: -Onone -Zdump=mir
//@[gas] compile-flags: -Ogas -Zdump=mir-final
//@[none] filecheck: --check-prefix=NONE
//@[gas] filecheck: --check-prefix=GAS
//@ normalize-stdout-test: "(?s).+" -> ""

// solc hashes everything except a string literal at run time when it does not optimize, so
// `-Onone` does too. Optimized builds fold every hash into one word. A constant and the hash of one
// fold at every level: uses cannot each run an initializer that names other constants.

// GAS-NOT: keccak256
contract C {
    // NONE-LABEL: fn @constructor{{[( ]}}
    // NONE: keccak256 {{v[0-9]+}}, {{v[0-9]+}}
    bytes32 private constant SLOT = keccak256("slot");
    string private constant TEXT = "a constant string that is longer than thirty-one bytes";
    uint256 private stored = uint256(keccak256(abi.encode(7)));

    // NONE-LABEL: fn @encoded{{[( ]}}
    // NONE: keccak256 {{v[0-9]+}}, {{v[0-9]+}}
    function encoded() external pure returns (bytes32) {
        return keccak256(abi.encode("a string that is longer than thirty-one bytes", 0));
    }

    // NONE-LABEL: fn @packed{{[( ]}}
    // NONE: keccak256_packed (uint256 5, uint256 6)
    function packed() external pure returns (bytes32) {
        return keccak256(abi.encodePacked(uint256(5), uint256(6)));
    }

    // NONE-LABEL: fn @erc7201{{[( ]}}
    // NONE: keccak256_packed (uint256 0x{{[0-9a-f]+}})
    function erc7201() external pure returns (uint256) {
        uint256 slot = uint256(keccak256(abi.encode(uint256(keccak256("namespace")) - 1))) & ~uint256(0xff);
        return slot;
    }

    // NONE-LABEL: fn @literal{{[( ]}}
    // NONE-NOT: keccak256
    // NONE: ret 0x{{[0-9a-f]+}}
    function literal() external pure returns (bytes32) {
        return keccak256("literal");
    }

    // NONE-LABEL: fn @constantBytes{{[( ]}}
    // NONE-NOT: keccak256
    // NONE: ret 0x{{[0-9a-f]+}}
    function constantBytes() external pure returns (bytes32) {
        return keccak256(bytes(TEXT));
    }

    // NONE-LABEL: fn @slot{{[( ]}}
    // NONE-NOT: keccak256
    // NONE: ret 0x{{[0-9a-f]+}}
    function slot() external pure returns (bytes32) {
        return SLOT;
    }

    function initializer() external view returns (uint256) {
        return stored;
    }
}
