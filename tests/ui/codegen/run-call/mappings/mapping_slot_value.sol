//@ codegen-matrix: standard
//@ run-call: check => 7

contract MappingSlotValue {
    mapping(uint256 => uint256) private values;

    function check() external returns (uint256) {
        values;
        values[1] = 7;
        return values[1];
    }
}
