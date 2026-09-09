//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@ run-call: get 0 => 0x0000000000000000000000000000000000000000, 0, 0, 0x0000000000000000000000000000000000000000
//@ run-call: get 1 => 0x0000000000000000000000000000000000000011, 22, 33, 0x0000000000000000000000000000000000000044
//@ run-call: get 2 => 0x0000000000000000000000000000000000000055, 66, 77, 0x0000000000000000000000000000000000000088

contract DisjointReturnStores {
    struct Item { address token; uint256 first; uint256 second; address owner; }
    mapping(uint256 => Item) private items;

    constructor() {
        items[1] = Item(address(0x11), 22, 33, address(0x44));
        items[2] = Item(address(0x55), 66, 77, address(0x88));
    }

    function get(uint256 key) external view returns (address, uint256, uint256, address) {
        Item storage item = items[key];
        return (item.token, item.first, item.second, item.owner);
    }
}
