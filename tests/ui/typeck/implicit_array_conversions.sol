//@compile-flags: -Ztypeck

// Tests for implicit array conversions.
// Arrays require exactly the same element type - no widening allowed.
// Fixed arrays must also have the same length.

contract C {
    // === Valid: same element type assignment ===
    function sameDynamicArray(uint256[] memory a) internal pure {
        uint256[] memory b = a;
    }

    function sameFixedArray(uint256[3] memory a) internal pure {
        uint256[3] memory b = a;
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

    // === Invalid: element type widening (NOT allowed for arrays) ===
    function wideningDynamic(uint8[] memory a) internal pure {
        uint256[] memory b = a; //~ ERROR: mismatched types
    }

    function wideningFixed(uint8[3] memory a) internal pure {
        uint256[3] memory b = a; //~ ERROR: mismatched types
    }

    // === Invalid: element type narrowing ===
    function narrowingDynamic(uint256[] memory a) internal pure {
        uint8[] memory b = a; //~ ERROR: mismatched types
    }
}
