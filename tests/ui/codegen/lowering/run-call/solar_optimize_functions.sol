//@ codegen-matrix: standard
//@ run-call: sum 4 => 10
//@ run-call: fast 4 => 10
//@ run-call: small 4 => 10
//@ run-call: viaInline 4 => 20
//@ run-call: pattern => 0x0101010101010101010101010101010101010101010101010101010101010101
//@ run-call-fail: share 0 => Panic(18)
//@ run-call: share 3 => 33
//@ run-call: tagged => 0x11223344, -5
//@ run-call: tagged_gas => 0x11223344

// A function tagged `@custom:solar-optimize` is compiled for its own objective in every build,
// the unoptimized one included, and computes the same results as untagged code.
contract Mixed {
    bytes4 immutable tag = 0x11223344;
    int16 immutable delta = -5;

    function sum(uint256 n) external pure returns (uint256 total) {
        for (uint256 i = 1; i <= n; i++) {
            total += i;
        }
    }

    /// @custom:solar-optimize gas
    function fast(uint256 n) external pure returns (uint256 total) {
        for (uint256 i = 1; i <= n; i++) {
            total += i;
        }
    }

    /// @custom:solar-optimize size
    function small(uint256 n) public pure returns (uint256 total) {
        for (uint256 i = 1; i <= n; i++) {
            total += i;
        }
    }

    /// @custom:solar-inline always
    /// @custom:solar-optimize gas
    function twice(uint256 n) internal pure returns (uint256) {
        return small(n) * 2;
    }

    function viaInline(uint256 n) external pure returns (uint256) {
        return twice(n);
    }

    /// @custom:solar-optimize gas
    function pattern() external pure returns (bytes32) {
        return 0x0101010101010101010101010101010101010101010101010101010101010101;
    }

    /// @custom:solar-optimize size
    function share(uint256 parts) external pure returns (uint256) {
        return 100 / parts;
    }

    // Immutables keep the layout of the contract, whatever the objective of the function that
    // reads them.
    /// @custom:solar-optimize size
    function tagged() external view returns (bytes4, int16) { //~ WARN: function state mutability can be restricted to pure
        return (tag, delta);
    }

    /// @custom:solar-optimize gas
    function tagged_gas() external view returns (bytes4) { //~ WARN: function state mutability can be restricted to pure
        return tag;
    }
}
