//@compile-flags: -Ztypeck

// Tests for implicit array conversions.
// Arrays allow element type widening if same signedness.
// Fixed arrays must also have same length.

contract C {
    // === Valid: same element type assignment ===
    function sameDynamicArray(uint256[] memory a) internal pure {
        uint256[] memory b = a;
    }

    function sameFixedArray(uint256[3] memory a) internal pure {
        uint256[3] memory b = a;
    }

    // === Valid: element type widening (same signedness) ===
    function wideningDynamic(uint8[] memory a) internal pure {
        uint256[] memory b = a; // OK - uint8 -> uint256
    }

    function wideningFixed(uint8[3] memory a) internal pure {
        uint256[3] memory b = a; // OK - uint8 -> uint256
    }

    function wideningBytes(bytes1[] memory a) internal pure {
        bytes32[] memory b = a; // OK - bytes1 -> bytes32
    }

    // === Invalid: different array lengths ===
    function differentLength(uint256[3] memory a) internal pure {
        uint256[4] memory b = a; //~ ERROR: mismatched types
    }

    // === Invalid: fixed to dynamic array ===
    function fixedToDynamic(uint256[3] memory a) internal pure {
        uint256[] memory b = a; //~ ERROR: mismatched types
    }

    // === Invalid: dynamic to fixed array ===
    function dynamicToFixed(uint256[] memory a) internal pure {
        uint256[3] memory b = a; //~ ERROR: mismatched types
    }

    // === Invalid: different signedness ===
    function signednessMismatch(int256[] memory a) internal pure {
        uint256[] memory b = a; //~ ERROR: mismatched types
    }

    // === Invalid: narrowing element type ===
    function narrowingDynamic(uint256[] memory a) internal pure {
        uint8[] memory b = a; //~ ERROR: mismatched types
    }
}
