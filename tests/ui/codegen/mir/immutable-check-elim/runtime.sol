//@ codegen-matrix: standard
//@ run-call: total(); constructor=[0, 0, 0] => 0
//@ run-call: total(); constructor=[18446744073709551615, 18446744073709551615, -1] => 36893488147419103230
//@ run-call: initial(); constructor=[18446744073709551615, 18446744073709551615, -1] => 36893488147419103230
//@ run-call: runtimeOnly(); constructor=[18446744073709551615, 0, 0] => 18446744073709551616
//@ run-call: signedPlusOne(); constructor=[0, 0, 7] => 8
//@ run-call-fail: signedPlusOne(); constructor=[0, 0, -1] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract ImmutableBounds {
    uint64 private immutable a;
    uint64 private immutable b;
    int64 private immutable signedValue;
    uint256 public immutable initial;

    constructor(uint64 x, uint64 y, int64 s) {
        a = x;
        b = y;
        signedValue = s;
        initial = total();
    }

    function total() public view returns (uint256) {
        return uint256(a) + uint256(b);
    }

    function runtimeOnly() public view returns (uint256) {
        return uint256(a) + 1;
    }

    function signedPlusOne() public view returns (uint256) {
        return uint256(int256(signedValue)) + 1;
    }
}
