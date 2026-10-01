//@ compile-flags: -Zdataflow=rounding
//@ filecheck:
// Rounding directions following Slither's rounding analysis. `mulDivUp` is recognized from
// its ceiling idiom and by name; truncating division rounds down. Adding values rounded in
// opposite directions, and dividing by a value rounded the same way as the numerator, make
// the error direction unknown.

// CHECK-LABEL: dataflow rounding (k=0): {{.*}}:Vault
// CHECK: fn @previewDeposit:
// CHECK: summary: ret0=down
// CHECK: fn @previewMint:
// CHECK: summary: ret0=up
// CHECK: fn @mixed:
// CHECK: summary: ret0=unknown
// CHECK: finding: rounding @mixed: {{.*}} combines values rounded up and down
// CHECK: finding: rounding @ratio: {{.*}} divides a value rounded down by a value rounded the same way
// CHECK: fn @ceil:
// CHECK: summary: ret0=up
library FixedPointMath {
    function mulDivUp(uint256 x, uint256 y, uint256 d) internal pure returns (uint256) {
        return (x * y + d - 1) / d;
    }

    function mulDivDown(uint256 x, uint256 y, uint256 d) internal pure returns (uint256) {
        return x * y / d;
    }
}

contract Vault {
    uint256 totalAssets;
    uint256 totalShares;

    function previewDeposit(uint256 assets) public view returns (uint256) {
        return FixedPointMath.mulDivDown(assets, totalShares, totalAssets);
    }

    function previewMint(uint256 shares) public view returns (uint256) {
        return FixedPointMath.mulDivUp(shares, totalAssets, totalShares);
    }

    function mixed(uint256 a, uint256 b) external view returns (uint256) {
        uint256 up = FixedPointMath.mulDivUp(a, totalShares, totalAssets);
        uint256 down = FixedPointMath.mulDivDown(b, totalShares, totalAssets);
        return up + down;
        //~^ WARN: rounding: `v6 = checked_add u256, v2, v5` combines values rounded up and down
    }

    function ratio(uint256 a) external view returns (uint256) {
        uint256 shares = FixedPointMath.mulDivDown(a, totalShares, totalAssets);
        uint256 assets = FixedPointMath.mulDivDown(a, totalAssets, totalShares);
        return shares / assets;
        //~^ WARN: rounding: `v6 = checked_div u256, v2, v5` divides a value rounded down by a value rounded the same way, which rounds the quotient the other way
    }

    function ceil(uint256 a, uint256 b) external pure returns (uint256) {
        return (a + b - 1) / b;
    }
}
