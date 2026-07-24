//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --enable-var-scope

contract BackendControlFlow {
    uint256 public value;
    uint256 public totalSupply;
    uint256 public reserve0;
    uint256 public reserve1;

    // CHECK-LABEL: fn @localVarInConditional
    // CHECK: [[VALUE:v[0-9]+]] = sload [[SLOT:[0-9]+]]
    // CHECK: br
    // CHECK: [[RESULT:v[0-9]+]] = sub [[VALUE]], 1
    // CHECK: sstore [[SLOT]], [[RESULT]]
    function localVarInConditional() public {
        uint256 current = value;
        if (current != 0) value = current - 1;
    }

    // CHECK-LABEL: fn @directStorageInConditional
    // CHECK: sload [[SLOT:[0-9]+]]
    // CHECK: br
    // CHECK: [[VALUE:v[0-9]+]] = sload [[SLOT]]
    // CHECK: [[RESULT:v[0-9]+]] = sub [[VALUE]], 1
    // CHECK: sstore [[SLOT]], [[RESULT]]
    function directStorageInConditional() public {
        if (value != 0) value = value - 1;
    }

    // CHECK-LABEL: fn @phiAfterBranch
    // CHECK: br
    // CHECK: [[LIQUIDITY:v[0-9]+]] = mload [[PHI_ADDR:[0-9]+]]
    // CHECK: [[SUPPLY:v[0-9]+]] = sload [[SUPPLY_SLOT:[0-9]+]]
    // CHECK: [[TOTAL:v[0-9]+]] = add [[SUPPLY]], [[LIQUIDITY]]
    // CHECK: sstore [[SUPPLY_SLOT]], [[TOTAL]]
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
    // CHECK: [[LIQUIDITY:v[0-9]+]] = mload [[PHI_ADDR:[0-9]+]]
    // CHECK: [[SUPPLY:v[0-9]+]] = sload [[SUPPLY_SLOT:[0-9]+]]
    // CHECK: [[TOTAL:v[0-9]+]] = add [[SUPPLY]], [[LIQUIDITY]]
    // CHECK: sstore [[SUPPLY_SLOT]], [[TOTAL]]
    // CHECK: [[FIRST_USE:v[0-9]+]] = mload [[PHI_ADDR]]
    // CHECK: [[TWICE:v[0-9]+]] = mul [[FIRST_USE]], 2
    // CHECK: [[SECOND_USE:v[0-9]+]] = mload [[PHI_ADDR]]
    // CHECK: [[RESULT:v[0-9]+]] = add [[TWICE]], [[SECOND_USE]]
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
    // CHECK: [[LIQUIDITY:v[0-9]+]] = mload [[RESULT_ADDR:[0-9]+]]
    // CHECK: [[SUPPLY:v[0-9]+]] = sload [[SUPPLY_SLOT:[0-9]+]]
    // CHECK: [[TOTAL:v[0-9]+]] = add [[SUPPLY]], [[LIQUIDITY]]
    // CHECK: [[FIRST_NUM:v[0-9]+]] = mul
    // CHECK: [[RESERVE0:v[0-9]+]] = sload [[RESERVE0_SLOT:[0-9]+]]
    // CHECK: [[FIRST:v[0-9]+]] = div [[FIRST_NUM]], [[RESERVE0]]
    // CHECK: [[SECOND_NUM:v[0-9]+]] = mul
    // CHECK: [[RESERVE1:v[0-9]+]] = sload [[RESERVE1_SLOT:[0-9]+]]
    // CHECK: [[SECOND:v[0-9]+]] = div [[SECOND_NUM]], [[RESERVE1]]
    // CHECK: lt [[FIRST]], [[SECOND]]
    // CHECK: mstore [[MERGE_ADDR:[0-9]+]], [[FIRST]]
    // CHECK: mstore [[MERGE_ADDR]], [[SECOND]]
    // CHECK: [[MIN:v[0-9]+]] = mload [[MERGE_ADDR]]
    // CHECK: mstore [[RESULT_ADDR]], [[MIN]]
    // CHECK: sstore [[SUPPLY_SLOT]], [[TOTAL]]
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
