//@ codegen-matrix: standard
//@ run-call: trimLen 0x010203 => 3
//@ run-call: trimLen 0x010203040506 => 2
//@ run-call: repeatedSourceJoin true, 7, 9 => 7, 7
//@ run-call: repeatedSourceJoin false, 7, 9 => 9, 7
//@ run-call: loopJoin 0 => 0
//@ run-call: loopJoin 4 => 12
//@ run-call: nestedLoops 3, 4 => 42
//@ run-call: conditionalSelfLoop 0, 4 => 0
//@ run-call: conditionalSelfLoop 3, 0 => 0
//@ run-call: conditionalSelfLoop 3, 4 => 42
//@ run-call: emptyExitSelfLoop 0 => 7
//@ run-call: emptyExitSelfLoop 4 => 7

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

    function loopJoin(uint256 n) external pure returns (uint256 result) {
        for (uint256 i; i < n; ++i) {
            uint256 value;
            if (i & 1 == 0) {
                value = i + 1;
            } else {
                value = i + 2;
            }
            result += value;
        }
    }

    function nestedLoops(uint256 outer, uint256 inner) external pure returns (uint256 result) {
        for (uint256 i; i < outer; ++i) {
            for (uint256 j; j < inner; ++j) {
                result += i + j + 1;
            }
        }
    }

    function conditionalSelfLoop(
        uint256 outer,
        uint256 inner
    ) external pure returns (uint256 result) {
        for (uint256 i; i < outer; ++i) {
            if (inner == 0) continue;
            uint256 j;
            do {
                unchecked {
                    result += i + j + 1;
                    ++j;
                }
            } while (j < inner);
        }
    }

    function emptyExitSelfLoop(uint256 n) external pure returns (uint256) {
        uint256 i;
        do {
            unchecked {
                ++i;
            }
        } while (i < n);
        return 7;
    }
}
