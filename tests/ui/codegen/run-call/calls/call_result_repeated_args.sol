//@ codegen-matrix: standard
//@ run-call: run => 94

contract CallResultRepeatedArgs {
    uint256 value = 42;

    function run() external returns (uint256 result) {
        store(1, 2);
        store(3, 4);
        uint256 kept = helper();
        assembly {
            calldatacopy(0, 0, codesize())
            if kept { sstore(1, kept) }
        }
        store(kept, kept);
        assembly { result := sload(2) }
    }

    function helper() internal view returns (uint256) {
        return value;
    }

    function store(uint256 a, uint256 b) internal {
        assembly {
            sstore(2, add(sload(2), add(a, b)))
            sstore(3, xor(a, b))
            sstore(4, mul(a, b))
            sstore(5, sub(a, b))
            sstore(6, or(a, b))
            sstore(7, and(a, b))
        }
    }

}
