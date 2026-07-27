//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=hashes -Zdump=evm-ir-runtime --pretty-json
//@ filecheck:

type Wad is uint256;

contract UdvtSelector {
    // CHECK: "unwrapAndAdd(uint256,uint256)": "8d2f9995"
    // CHECK: @module runtime
    // CHECK: push 0x8d2f9995
    // CHECK: push 36
    // CHECK-NEXT: calldataload
    // CHECK: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: add
    // CHECK: return
    function unwrapAndAdd(Wad x, uint256 y) external pure returns (uint256) {
        return Wad.unwrap(x) + y;
    }
}
