//@ codegen-matrix: standard
//@ run-call: length false => 1
//@ run-call: length true => 0
//@ run-call: lengthAfterDirtyScratch => 0

contract DeleteMemoryLocal {
    function length(bool clear) external pure returns (uint256) {
        uint256[] memory values = new uint256[](1);
        values[0] = 7;
        if (clear) delete values;
        return values.length;
    }

    function lengthAfterDirtyScratch() external pure returns (uint256) {
        uint256[] memory values = new uint256[](1);
        assembly {
            mstore(0, not(0))
        }
        delete values;
        return values.length;
    }
}
