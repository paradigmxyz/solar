contract OversizedStorageCopy {
    uint256[1 << 64] private source;
    uint256[1 << 64] private target;

    function copy() external {
        target = source; //~ ERROR: array is too large to copy or encode
    }

    function tupleCopy() external {
        (target, target) = (source, source);
        //~^ ERROR: array is too large to copy or encode
        //~| ERROR: array is too large to copy or encode
    }

    function memoryCopy() external view returns (bytes memory) {
        return abi.encode(source);
        //~^ ERROR: array is too large to copy or encode
    }
}

contract OversizedStructCopy {
    struct Data {
        uint256[1 << 64] values;
    }

    Data private source;
    Data private target;

    function copy() external {
        target = source; //~ ERROR: array is too large to copy or encode
    }
}

contract NestedOversizedCopy {
    uint256[1 << 64][] source;
    uint256[1 << 64][] target;

    function copy() external {
        target = source; //~ ERROR: array is too large to copy or encode
    }

    function tupleReturn() external view returns (uint256[1 << 64][] memory, uint256) {
        return (source, 1); //~ ERROR: array is too large to copy or encode
    }

    function push() external {
        target.push(source[0]); //~ ERROR: array is too large to copy or encode
    }
}

contract OversizedStorageReferences {
    struct Node {
        Node[] children;
        uint256[1 << 64] values;
    }
    struct Mapped {
        mapping(uint256 => uint256[1 << 64]) values;
    }
    Node source;
    Node target;
    Mapped mapped;

    function copy() external {
        target = source; //~ ERROR: array is too large to copy or encode
    }

    function references() external {
        uint256[1 << 64] storage values = source.values;
        values = target.values;
        values[0] = 1;
        Mapped storage ref = mapped;
        ref.values[0][0] = 2;
    }
}
