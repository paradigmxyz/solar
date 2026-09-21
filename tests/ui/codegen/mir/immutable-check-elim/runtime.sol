//@ codegen-matrix: standard
//@ run-call: total(); constructor=[0, 0, 0] => 0
//@ run-call: narrowed(); constructor=[255, 0, 0] => 256
//@ run-call: narrowThreeBytes(); constructor=[255, 0, 0] => 256
//@ run-call: narrowed(); constructor=[256, 0, 0] => 1
//@ run-call: narrowInitial(); constructor=[255, 0, 0] => 256
//@ run-call: narrowWide(); constructor=[0, 65535, 0] => 65536
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
    uint64 private immutable narrow;
    uint24 private immutable threeBytes;
    uint160 private immutable narrowAddressWidth;
    uint64 public immutable narrowInitial;

    constructor(uint64 x, uint64 y, int64 s) {
        narrow = uint8(x);
        threeBytes = uint8(x);
        narrowAddressWidth = uint16(y);
        narrowInitial = narrow + 1;
        a = x;
        b = y;
        signedValue = s;
        initial = total();
    }

    function narrowThreeBytes() public view returns (uint24) {
        return threeBytes + 1;
    }

    function narrowed() public view returns (uint64) {
        return narrow + 1;
    }

    function narrowWide() public view returns (uint160) {
        return narrowAddressWidth + 1;
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
