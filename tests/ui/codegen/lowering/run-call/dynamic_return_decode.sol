//@ codegen-matrix: standard
//@ run-call: ArrayCaller::arrLenAndSum => 2, 15
//@ run-call: StringCaller::strFirstAndLen => 0x68, 5
//@ run-call: BytesCaller::bytesRoundTrip => 2
//@ run-call: FixedCaller::fixedSum => 60
//@ run-call: StringElementCaller::stringElem => 5
//@ run-call: NestedCaller::nestedSum => 7

// External-call returns of dynamic types are ABI blobs whose head words are
// payload-relative offsets: they must be decoded, not used as memory
// pointers. Storage arrays used as values materialize fresh memory copies,
// including `bytes`/`string` and nested-array elements.
//
// The nested fixture is built from a memory array: pushing a storage array
// into another storage array chains the materialization loop into the
// storage-copy loop, and the backend's spill-slot sharing miscompiles that
// shape (a phi-edge copy reads a slot still holding the allocation size).

contract Callee {
    uint256[] internal xs;
    uint256[3] internal fixedXs = [10, 20, 30];
    string[] internal names;
    uint256[][] internal nested;

    constructor() {
        xs.push(7);
        xs.push(8);
        names.push("hello");
        uint256[] memory inner = new uint256[](3);
        inner[0] = 7;
        inner[1] = 8;
        inner[2] = 9;
        nested.push(inner);
    }

    function getXs() external view returns (uint256[] memory) {
        return xs;
    }

    function getStr() external pure returns (string memory) {
        return "hello";
    }

    function getBytes() external pure returns (bytes memory) {
        return hex"aabb";
    }

    function getFixed() external view returns (uint256[3] memory) {
        return fixedXs;
    }

    function getName() external view returns (string memory) {
        return names[0];
    }

    function getNested() external view returns (uint256[][] memory) {
        return nested;
    }
}

contract ArrayCaller {
    function arrLenAndSum() public returns (uint256 len, uint256 sum) {
        uint256[] memory m = new Callee().getXs();
        len = m.length;
        for (uint256 i; i < m.length; i++) {
            sum += m[i];
        }
    }
}

contract StringCaller {
    function strFirstAndLen() public returns (bytes1 first, uint256 len) {
        string memory s = new Callee().getStr();
        first = bytes(s)[0];
        len = bytes(s).length;
    }
}

contract BytesCaller {
    function bytesRoundTrip() public returns (uint256) {
        return new Callee().getBytes().length;
    }
}

contract FixedCaller {
    function fixedSum() public returns (uint256 sum) {
        uint256[3] memory m = new Callee().getFixed();
        sum = m[0] + m[1] + m[2];
    }
}

contract StringElementCaller {
    function stringElem() public returns (uint256) {
        return bytes(new Callee().getName()).length;
    }
}

contract NestedCaller {
    function nestedSum() public returns (uint256) {
        uint256[][] memory n = new Callee().getNested();
        return n[0][0];
    }
}
