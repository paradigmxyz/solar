// Index range access needs a statically encoded base type: the element offset
// is relative to the slice start, which a dynamically encoded base cannot
// express. The base type of a calldata array is a calldata reference, so the
// rule has to look through the data location.

struct Dynamic {
    uint256[] a;
}

struct Static {
    uint256 a;
}

struct Nested {
    Dynamic d;
}

contract C {
    function structBase(Dynamic[] calldata d) external pure returns (uint256) {
        return d[1:2][0].a.length; //~ ERROR: index range access is not supported for arrays with dynamically encoded base types
    }

    function structBaseToMemory(Dynamic[] calldata d) external pure {
        Dynamic[] memory m = d[0:1]; //~ ERROR: index range access is not supported for arrays with dynamically encoded base types
        m;
    }

    function nestedStructBase(Nested[] calldata d) external pure {
        d[0:1]; //~ ERROR: index range access is not supported for arrays with dynamically encoded base types
    }

    function arrayBase(uint256[][] calldata d) external pure {
        d[0:1]; //~ ERROR: index range access is not supported for arrays with dynamically encoded base types
    }

    function fixedArrayOfDynamicBase(uint256[][2][] calldata d) external pure {
        d[0:1]; //~ ERROR: index range access is not supported for arrays with dynamically encoded base types
    }

    // A statically encoded base is fine, in calldata and copied to memory.
    function staticStructBase(Static[] calldata d) external pure returns (uint256) {
        Static[] memory m = d[0:1];
        return m.length + d[0:1][0].a;
    }

    function fixedArrayBase(uint256[2][] calldata d) external pure {
        uint256[2][] memory m = d[0:1];
        m;
    }
}
