//@ revisions: size gas
//@[size] compile-flags: -Osize -Zdump=mir
//@[gas] compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// A third call passes a pointer assembly made up, which can wrap to the start of calldata.
// CHECK-LABEL: fn @e
// CHECK-NOT: calldatacopy {{v[0-9]+}}, arg0

struct Item { address a; uint256 b; uint256 c; uint256 d; uint256 e; }
contract Test {
    function e(Item calldata it, uint256 k) internal pure returns (bytes memory) { return abi.encode(it, k); }
    function f1(Item calldata it, bytes calldata d) external pure returns (bytes memory) { return e(it, d.length + 1); }
    function f2(Item calldata it, bytes calldata d) external pure returns (bytes memory) { return e(it, d.length + 2); }
    function f3(uint256 p, bytes calldata d) external pure returns (bytes memory) {
        Item calldata raw;
        assembly { raw := p }
        return e(raw, d.length + 3);
    }
}
