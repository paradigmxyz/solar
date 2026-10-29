//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// A literal that the target stores as words takes no object of its own. One it copies from the
// data section, or one of zeros, keeps its object, as the encoder cannot fold that into stores.
contract EventLiteralGate {
    event Named(string name, uint256 value);

    // CHECK-LABEL: fn @copied(
    // CHECK: datacopy
    function copied() external {
        emit Named("standardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWad", 1);
    }

    // CHECK-LABEL: fn @stored(
    // CHECK-NOT: mcopy
    // CHECK-NOT: datacopy
    // CHECK: log
    function stored() external {
        emit Named("standardNormalWadStandardNormalWad", 1);
    }

    // CHECK-LABEL: fn @zeros(
    // CHECK: calldatacopy
    // CHECK: log
    function zeros() external {
        emit Named("\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00", 1);
    }
}
