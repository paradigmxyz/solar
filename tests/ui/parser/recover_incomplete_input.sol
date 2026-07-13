//@ revisions: strict recover
//@[recover] compile-flags: -Zrecover-incomplete-input

contract C {
    function target(uint256 amount, address account) internal returns (uint256) {
        return amount + uint256(uint160(account));
    }

    function use() internal returns (uint256) {
        return target(1, //~[recover] ERROR: wrong argument count
    } //~ ERROR: expected one of

    function later() internal {
        uint8 value = 300; //~[recover] ERROR: mismatched types
    } //~[recover] ERROR: expected contract item
