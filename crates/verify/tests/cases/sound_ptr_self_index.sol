pragma solidity ^0.8.20;
contract SelfIndex {
    struct S { uint256 x; uint256 y; }
    S[] arr;

    // Solidity: the write through p lands in the last element, so it is 5.
    /// @custom:ensures arr.length >= 1 && arr[arr.length - 1].x == 0
    function bad_write_lost() external {
        arr.push();
        S storage p = arr[arr.length - 1];
        p.x = 5;
    }
    /// @custom:ensures arr.length >= 1 && arr[arr.length - 1].x == 5
    function ok_write_kept() external {
        arr.push();
        S storage p = arr[arr.length - 1];
        p.x = 5;
    }

    // Solidity: p reads the element the direct write changed, so result is 7.
    /// @custom:ensures result == 0
    function bad_read_stale() external returns (uint256 result) {
        arr.push();
        S storage p = arr[arr.length - 1];
        arr[arr.length - 1].x = 7;
        return p.x;
    }
    /// @custom:ensures result == 7
    function ok_read_fresh() external returns (uint256 result) {
        arr.push();
        S storage p = arr[arr.length - 1];
        arr[arr.length - 1].x = 7;
        return p.x;
    }
}
