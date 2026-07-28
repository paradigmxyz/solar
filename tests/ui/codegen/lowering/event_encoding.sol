//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract EventEncoding {
    event ArrayEvent(uint256[2] values);
    event AnonymousEvent(address indexed sender, uint256 value) anonymous;

    // CHECK-LABEL: fn @emitArray
    // CHECK: log1 {{[^,]+}}, 64, 0x625921711aa49386aa5b640487cbc3efdcbda2656254ae2c5ad71b2ede1efcf4
    function emitArray(uint256[2] memory values) external {
        emit ArrayEvent(values);
    }

    // CHECK-LABEL: fn @emitAnonymous
    // CHECK: log1 {{[^,]+}}, 32, arg0
    function emitAnonymous(address sender, uint256 value) external {
        emit AnonymousEvent(sender, value);
    }
}
