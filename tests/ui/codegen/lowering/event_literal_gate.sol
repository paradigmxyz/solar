//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// A literal written as words takes no object of its own, however long it is. One of zeros keeps
// its object, as the encoder cannot fold that into stores.
contract EventLiteralGate {
    event Named(string name, uint256 value);

    // CHECK-LABEL: fn @long(
    // CHECK-NOT: mcopy
    // CHECK-NOT: datacopy
    // CHECK: log
    function long() external {
        emit Named("standardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWad", 1);
    }

    // CHECK-LABEL: fn @short(
    // CHECK-NOT: mcopy
    // CHECK-NOT: datacopy
    // CHECK: log
    function short() external {
        emit Named("standardNormalWadStandardNormalWad", 1);
    }

    // CHECK-LABEL: fn @zeros(
    // CHECK: mcopy
    // CHECK: log
    function zeros() external {
        emit Named("\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00", 1);
    }
}
