# Gas Comparison: Solar vs solc (with and without optimizer)

## Summary

**Only `multi-return` tests pass with Solar codegen.** Other tests fail with `StackUnderflow` errors.

## multi-return Contract Results

### Deployment Cost

| Compiler          | Deployment Cost | Deployment Size |
|-------------------|-----------------|-----------------|
| solc (no opt)     | 401,888         | 1,640 bytes     |
| solc (opt=200)    | 259,540         | 982 bytes       |
| Solar (no opt)    | 126,136         | 640 bytes       |
| Solar (opt=200)   | 126,136         | 640 bytes       |

**Solar deployment is 3.2x cheaper than unoptimized solc, 2x cheaper than optimized solc.**

### Function Gas Costs

| Function      | solc (no opt) | solc (opt=200) | Solar (no opt) | Solar (opt=200) |
|---------------|---------------|----------------|----------------|-----------------|
| getTwo        | 510           | 284            | 21,160         | 21,160          |
| getThree      | 557           | 218            | 21,182         | 21,182          |
| simpleReturn  | 488           | 262            | 21,258         | 21,258          |
| callVia       | 4,487         | 3,585          | N/A (fails)    | N/A (fails)     |

**Note:** Solar function execution costs are ~40x higher than solc. This appears to be a codegen issue with base transaction overhead (21,000 gas) being included.

## Other Tests Status

| Test Directory    | solc  | Solar |
|-------------------|-------|-------|
| arithmetic        | ✅    | ❌ StackUnderflow |
| calls             | ✅    | ❌ StackUnderflow |
| constructor-args  | ✅    | ❌ compile fails  |
| control-flow      | ✅    | ❌ StackUnderflow |
| events            | ✅    | ❌ compile fails  |
| multi-return      | ✅    | ⚠️ partial (callVia fails) |
| stack-deep        | ✅    | ❌ StackUnderflow |
| storage           | ✅    | ❌ compile fails  |

## Observations

1. **Solar optimizer has no effect** - Gas costs are identical with and without optimizer flag
2. **Deployment size is significantly smaller** with Solar (640 vs 982-1640 bytes)
3. **Runtime gas is much higher** in Solar due to apparent base cost inclusion
4. **Most tests fail** due to StackUnderflow or compilation errors in Solar codegen
