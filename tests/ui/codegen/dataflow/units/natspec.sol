//@ compile-flags: -Zdataflow=units
//@ filecheck:
// Dimensional analysis seeded from NatSpec. Multiplication adds scales and multiplies units,
// division by a power of ten rescales, and adding amounts of different units is reported.
// Unannotated values stay unknown and never produce findings.

// CHECK: fn @value:
// CHECK: checked_mul u256, arg0, arg1  ; D26{UoA}
// CHECK: checked_div u256, v0, 0x5f5e100  ; D18{UoA}
// CHECK: summary: ret0=D18{UoA}
// CHECK: fn @shares:
// CHECK: summary: ret0=D18{share}
// CHECK: finding: units @mixed: {{.*}} combines D18{tok} with D18{share}
// CHECK: fn @unannotated:
// CHECK-NOT: finding:
contract Units {
    /// @param amount D18{tok}
    /// @param price D8{UoA/tok}
    function value(uint256 amount, uint256 price) external pure returns (uint256) {
        return amount * price / 1e8;
    }

    /// @param assets D18{tok}
    /// @param rate D18{share/tok}
    /// @return D18{share}
    function shares(uint256 assets, uint256 rate) public pure returns (uint256) {
        return assets * rate / 1e18;
    }

    /// @param amount D18{tok}
    /// @param bonus D18{share}
    function mixed(uint256 amount, uint256 bonus) external pure returns (uint256) {
        return amount + bonus;
        //~^ WARN: units: `v0 = checked_add u256, arg0, arg1` combines D18{tok} with D18{share}
    }

    function unannotated(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b + 1;
    }
}
