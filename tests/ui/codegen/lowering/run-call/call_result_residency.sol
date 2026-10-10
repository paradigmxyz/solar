//@ codegen-matrix: standard
//@ run-call: read => 84

contract CallResultResidency {
    uint256 value = 42;

    constructor() {
        uint256 kept = helper();
        assembly {
            calldatacopy(0, 0, codesize())
            if kept { sstore(1, kept) }
            sstore(2, kept)
        }
    }

    function helper() internal view returns (uint256) {
        return value;
    }

    function read() external view returns (uint256 result) {
        assembly {
            result := add(sload(1), sload(2))
        }
    }
}
