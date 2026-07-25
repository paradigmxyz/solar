//@ run-call: trimLen(bytes) 0x010203 => 3
//@ run-call: trimLen(bytes) 0x010203040506 => 2
//@ run-call: repeatedSourceJoin(bool,uint256,uint256) true, 7, 9 => 7, 7
//@ run-call: repeatedSourceJoin(bool,uint256,uint256) false, 7, 9 => 9, 7

contract AcyclicStackPhi {
    function trimLen(bytes calldata data) external pure returns (uint256) {
        return trim(data).length;
    }

    function trim(bytes calldata data) internal pure returns (bytes calldata) {
        if (data.length > 4) return data[4:];
        return data;
    }

    function repeatedSourceJoin(
        bool first,
        uint256 a,
        uint256 b
    ) external pure returns (uint256 x, uint256 y) {
        if (first) {
            x = a;
            y = a;
        } else {
            x = b;
            y = a;
        }
    }
}
