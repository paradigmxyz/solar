//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=CHECK --enable-var-scope

contract BackendControlFlow {
    uint256 public value;
    uint256 public totalSupply;
    uint256 public reserve0;
    uint256 public reserve1;

    // CHECK-LABEL: fn @localVarInConditional
    // CHECK: [[VALUE:v[0-9]+]] = sload 0
    // CHECK: br
    // CHECK: sub [[VALUE]], 1
    // CHECK: sstore 0,
    function localVarInConditional() public {
        uint256 current = value;
        if (current != 0) value = current - 1;
    }

    // CHECK-LABEL: fn @directStorageInConditional
    // CHECK: sload 0
    // CHECK: br
    // CHECK: [[VALUE:v[0-9]+]] = sload 0
    // CHECK: sub [[VALUE]], 1
    // CHECK: sstore 0,
    function directStorageInConditional() public {
        if (value != 0) value = value - 1;
    }

    // CHECK-LABEL: fn @phiAfterBranch
    // CHECK: br
    // CHECK: [[LIQUIDITY:v[0-9]+]] = mload 128
    // CHECK: add {{.*}}, [[LIQUIDITY]]
    function phiAfterBranch() external returns (uint256 liquidity) {
        if (totalSupply == 0) {
            liquidity = 1;
        } else {
            liquidity = 2;
        }
        totalSupply += liquidity;
    }

    // CHECK-LABEL: fn @phiUsedMultipleTimes
    // CHECK: br
    // CHECK: sstore 1,
    // CHECK: [[FIRST_USE:v[0-9]+]] = mload 160
    // CHECK: [[TWICE:v[0-9]+]] = mul [[FIRST_USE]], 2
    // CHECK: [[SECOND_USE:v[0-9]+]] = mload 160
    // CHECK: add [[TWICE]], [[SECOND_USE]]
    function phiUsedMultipleTimes() external returns (uint256 result) {
        uint256 liquidity;
        if (totalSupply == 0) {
            liquidity = 1;
        } else {
            liquidity = 2;
        }
        totalSupply += liquidity;
        uint256 twice = liquidity * 2;
        result = twice + liquidity;
    }

    // CHECK-LABEL: fn @phiWithTernary
    // CHECK: br
    // CHECK: [[LIQUIDITY:v[0-9]+]] = mload 128
    // CHECK: add {{.*}}, [[LIQUIDITY]]
    // CHECK: [[RESERVE0:v[0-9]+]] = sload 2
    // CHECK: [[FIRST:v[0-9]+]] = div {{.*}}, [[RESERVE0]]
    // CHECK: [[RESERVE1:v[0-9]+]] = sload 3
    // CHECK: [[SECOND:v[0-9]+]] = div {{.*}}, [[RESERVE1]]
    // CHECK: lt [[FIRST]], [[SECOND]]
    // CHECK: mstore 0, [[FIRST]]
    // CHECK: mstore 0, [[SECOND]]
    // CHECK: [[MIN:v[0-9]+]] = mload 0
    // CHECK: mstore 128, [[MIN]]
    function phiWithTernary() external returns (uint256 liquidity) {
        uint256 amount0 = 100;
        uint256 amount1 = 200;

        if (totalSupply == 0) {
            liquidity = amount0 * amount1;
        } else {
            uint256 first = (amount0 * totalSupply) / reserve0;
            uint256 second = (amount1 * totalSupply) / reserve1;
            liquidity = first < second ? first : second;
        }

        totalSupply += liquidity;
    }
}
