//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck: --enable-var-scope

contract BackendControlFlow {
    uint256 public value;
    uint256 public totalSupply;
    uint256 public reserve0;
    uint256 public reserve1;

    // CHECK-LABEL: fn @localVarInConditional
    // CHECK: [[VALUE:v[0-9]+]] = sload [[SLOT:[0-9]+]]
    // CHECK: jumpi
    // CHECK: [[RESULT:v[0-9]+]] = sub [[VALUE]], 1
    // CHECK: sstore [[SLOT]], [[RESULT]]
    function localVarInConditional() public {
        uint256 current = value;
        if (current != 0) value = current - 1;
    }

    // CHECK-LABEL: fn @directStorageInConditional
    // CHECK: sload [[SLOT:[0-9]+]]
    // CHECK: jumpi
    // CHECK: [[VALUE:v[0-9]+]] = sload [[SLOT]]
    // CHECK: [[RESULT:v[0-9]+]] = sub [[VALUE]], 1
    // CHECK: sstore [[SLOT]], [[RESULT]]
    function directStorageInConditional() public {
        if (value != 0) value = value - 1;
    }

    // CHECK-LABEL: fn @phiAfterBranch
    // CHECK: jumpi
    // CHECK: phi [
    // CHECK: sload 1
    // CHECK: add
    // CHECK: sstore 1,
    function phiAfterBranch() external returns (uint256 liquidity) {
        if (totalSupply == 0) {
            liquidity = 1;
        } else {
            liquidity = 2;
        }
        totalSupply += liquidity;
    }

    // CHECK-LABEL: fn @phiUsedMultipleTimes
    // CHECK: jumpi
    // CHECK: phi [
    // CHECK: sload 1
    // CHECK: add
    // CHECK: sstore 1,
    // CHECK: mul
    // CHECK: add
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
    // CHECK: jumpi
    // CHECK: phi [
    // CHECK: sload 1
    // CHECK: sstore 1,
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
