// Tests for implicit array conversions.
// Outside direct storage copies, arrays require exactly the same element type and length.

contract C {
    struct Holder {
        uint32[] children;
        uint32[][] arrays;
    }

    Holder holder;
    uint256[] storageArr;
    uint32[] widerStorageArr;
    uint8[] narrowerStorageArr;
    uint32[4] widerFixedStorageArr;
    uint8[3] narrowerFixedStorageArr;
    uint256[][] nestedDynamicStorageArr;
    uint256[4][] nestedFixedStorageArr;
    uint256[2] shortFixedStorageArr;
    uint256[3] longFixedStorageArr;
    mapping(int256 => int256) mappingArrElement;

    // === Valid: same element type assignment ===
    function sameDynamicArray(uint256[] memory a) internal pure {
        uint256[] memory b = a;
    }

    function sameFixedArray(uint256[3] memory a) internal pure {
        uint256[3] memory b = a;
    }

    // === Valid: element-wise copies into direct storage arrays ===

    function wideningStorageCopy() internal {
        widerStorageArr = narrowerStorageArr;
        widerFixedStorageArr = narrowerFixedStorageArr;
        longFixedStorageArr = shortFixedStorageArr;
        shortFixedStorageArr = longFixedStorageArr; //~ ERROR: mismatched types
    }

    function nestedStorageCopies(
        uint256[][] memory dynamicSource,
        uint256[2][] memory fixedSource,
        uint256[2][3] memory fixedToDynamicSource
    ) internal {
        nestedDynamicStorageArr = dynamicSource;
        nestedFixedStorageArr = fixedSource;
        nestedDynamicStorageArr = fixedToDynamicSource;
    }

    function tupleStorageCopies(
        uint256[][] memory dynamicSource,
        uint256[2][] memory fixedSource
    ) internal {
        (nestedDynamicStorageArr, nestedFixedStorageArr) = (dynamicSource, fixedSource);
    }

    function storagePointerMemberCopies(uint8[] memory source) internal {
        Holder storage pointer = holder;
        pointer.children = source;
        pointer.arrays[0] = source;
    }

    function memoryToStoragePointer(uint8[] memory a) internal {
        uint32[] storage b = a; //~ ERROR: mismatched types
    }

    function memoryToStorageParameter(
        uint32[] storage pointer,
        uint8[] memory a,
        uint32[] memory exact
    ) internal {
        pointer = a; //~ ERROR: mismatched types
        (pointer) = exact; //~ ERROR: mismatched types
    }

    function tupleMemoryToStorageParameter(
        uint32[] storage pointer,
        uint8[] memory a,
        uint256 value
    ) internal {
        (pointer, value) = (a, value); //~ ERROR: mismatched types
    }

    function fixedArrayLiteral() internal pure {
        uint256[3] memory a = [uint256(1), uint256(2), uint256(3)];
        sameFixedArray([uint256(1), uint256(2), uint256(3)]);
    }

    // === Valid: explicit conversions preserve data locations ===

    function explicitMemoryArray(uint256[] memory a) internal pure returns (uint256[] memory) {
        return uint256[](a);
    }

    function explicitCalldataArray(uint256[] calldata a) external pure returns (uint256[] memory) {
        return uint256[](a);
    }

    function explicitStorageArray() internal view returns (uint256) {
        uint256[] storage a = storageArr;
        uint256[] storage b = uint256[](a);
        return b.length;
    }

    function explicitFixedArray(uint256[3] memory a) internal pure returns (uint256[3] memory) {
        return uint256[3](a);
    }

    // === Invalid: explicit array conversions must preserve shape and element type ===

    function explicitFixedToDynamic(uint256[3] memory a) internal pure returns (uint256[] memory) {
        return uint256[](a); //~ ERROR: invalid explicit type conversion
    }

    function explicitDynamicToFixed(uint256[] memory a) internal pure returns (uint256[3] memory) {
        return uint256[3](a); //~ ERROR: invalid explicit type conversion
    }

    function explicitElementMismatch(uint8[] memory a) internal pure returns (uint256[] memory) {
        return uint256[](a); //~ ERROR: invalid explicit type conversion
    }

    function explicitMemoryToCalldata(uint256[] memory a) external pure {
        uint256[] calldata b = uint256[](a); //~ ERROR: mismatched types
    }

    function explicitStorageToCalldata() external view {
        uint256[] storage a = storageArr;
        uint256[] calldata b = uint256[](a); //~ ERROR: mismatched types
    }

    // === Invalid: inline array element types must be mobile and nameable ===
    function invalidMobileType() internal pure {
        [uint256]; //~ ERROR: invalid mobile type
    }

    function unnamedTupleElement() internal pure {
        [(uint256(1), uint256(2)), (uint256(3), uint256(4))]; //~ ERROR: cannot infer nameable array element type
    }

    function mappingElementType() internal view {
        [mappingArrElement]; //~ ERROR: is only valid in storage because it contains a (nested) mapping
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
