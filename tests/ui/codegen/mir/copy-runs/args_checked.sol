//@ revisions: size gas
//@[size] compile-flags: -Osize -Zdump=mir
//@[gas] compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// Both calls of `e` pass a pointer the checks of its caller keep inside calldata, so the struct
// it encodes cannot wrap and is one copy.
// CHECK-LABEL: fn @e
// CHECK: calldatacopy {{v[0-9]+}}, arg0, 160

struct Item { address a; uint256 b; uint256 c; uint256 d; uint256 e; }
contract Test {
    function e(Item calldata it, uint256 k) internal pure returns (bytes memory) { return abi.encode(it, k); }
    function f1(Item calldata it, bytes calldata d) external pure returns (bytes memory) { return e(it, d.length + 1); }
    function f2(Item calldata it, bytes calldata d) external pure returns (bytes memory) { return e(it, d.length + 2); }
}
