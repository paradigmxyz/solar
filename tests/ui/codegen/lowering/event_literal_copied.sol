//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// A literal that the target copies from the data section instead of storing as words keeps its
// object, as the encoder cannot fold the copy into stores.
contract EventLiteralCopied {
    event Named(string name, uint256 value);

    // CHECK-LABEL: fn @copied(
    // CHECK: alloc
    // CHECK: datacopy
    function copied() external {
        emit Named("standardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWadstandardNormalWad", 1);
    }
}
