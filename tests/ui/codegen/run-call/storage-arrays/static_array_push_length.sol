//@ codegen-matrix: standard
//@ run-call: pushLength => 3, 1
//@ run-call: discardLength => 1

contract StaticArrayPushLength {
    uint256[3][] values;

    function pushLength() external returns (uint256, uint256) {
        uint256 length = values.push().length;
        return (length, values.length);
    }

    function discardLength() external returns (uint256) {
        values.push().length;
        return values.length;
    }
}
