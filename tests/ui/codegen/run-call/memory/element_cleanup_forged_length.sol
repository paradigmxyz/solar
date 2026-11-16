//@ codegen-matrix: standard
//@ run-call: forged 3618502788666131106986593281521497120414687020801267626233049500247285301247 => 255

// Assembly can forge a fresh array's length so that a wrapping index reads the header word as an
// element. The element read still masks that word to the element type.
contract ElementCleanupForgedLength {
    function forged(uint256 i) external pure returns (uint256) {
        uint8[] memory a = new uint8[](1);
        assembly {
            mstore(a, not(0))
        }
        return a[i];
    }
}
