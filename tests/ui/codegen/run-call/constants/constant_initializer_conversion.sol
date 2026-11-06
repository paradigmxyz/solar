//@ codegen-matrix: standard
//@ run-call: negative => -5
//@ run-call: allOnes => -1
//@ run-call: narrowUnsigned => 255
// A constant converts its initializer to its declared type, also when the initializer does not
// fold to a value.

int256 constant K = int16(-5);
int256 constant K3 = int8(int256(-1));
uint256 constant U = uint8(type(uint8).max);

contract C {
    function negative() external pure returns (int256) {
        return K;
    }

    function allOnes() external pure returns (int256) {
        return K3;
    }

    function narrowUnsigned() external pure returns (uint256) {
        return U;
    }
}
