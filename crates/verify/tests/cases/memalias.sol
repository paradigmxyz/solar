pragma solidity ^0.8.20;
/// Memory structs and arrays are references: a write through one that shares its memory with
/// another must not verify as if it were a copy.
contract MemAlias {
    struct S { uint256 x; }
    struct P { S inner; uint256[] a; }

    function _set(uint256[] memory a) internal pure { a[0] = 5; }
    function _id(S memory s) internal pure returns (S memory) { return s; }

    /// @custom:ensures result == 0
    function bad_copy_struct() external pure returns (uint256) {
        S memory a = S(0);
        S memory b = a;
        b.x = 5;
        return a.x;
    }
    /// @custom:ensures result == 0
    function bad_copy_array() external pure returns (uint256) {
        uint256[] memory a = new uint256[](1);
        uint256[] memory b = a;
        b[0] = 5;
        return a[0];
    }
    /// @custom:ensures result == 0
    function bad_callee_writes() external pure returns (uint256) {
        uint256[] memory a = new uint256[](1);
        _set(a);
        return a[0];
    }
    /// @custom:ensures result == 0
    function bad_returned_alias() external pure returns (uint256) {
        S memory a = S(0);
        S memory b = _id(a);
        b.x = 5;
        return a.x;
    }
    /// @custom:ensures result == 0
    function bad_field_alias() external pure returns (uint256) {
        P memory p;
        S memory s = p.inner;
        s.x = 5;
        return p.inner.x;
    }
    /// @custom:ensures result == 0
    function bad_container_alias() external pure returns (uint256) {
        S[] memory arr = new S[](2);
        S memory s = S(0);
        arr[1] = s;
        arr[1].x = 5;
        return s.x;
    }
    /// @custom:ensures result == 0
    function bad_loop_alias(uint256 n) external pure returns (uint256) {
        S memory a = S(0);
        S memory b = S(0);
        /// @custom:invariant true
        for (uint256 i; i < n; ++i) {
            a.x = 5;
            b = a;
        }
        b.x = 0;
        return a.x;
    }
    function _two(S memory a, S memory b) internal pure { a.x = 1; b.x = b.x + 5; }
    /// @custom:ensures result == 5
    function bad_same_object_twice() external pure returns (uint256 result) {
        S memory s = S(0);
        _two(s, s);
        result = s.x;
    }
    function _keep(S memory a, P memory p) internal pure { p.inner.x = 9; a.x = a.x + 1; }
    /// @custom:ensures result == 1
    function bad_inner_and_outer() external pure returns (uint256 result) {
        P memory p;
        _keep(p.inner, p);
        result = p.inner.x;
    }
    /// @custom:ensures result == v
    function ok_write_then_alias(uint256 v) external pure returns (uint256) {
        S[] memory arr = new S[](2);
        S memory s;
        s.x = v;
        arr[1] = s;
        return arr[1].x;
    }
    /// @custom:ensures result == v
    function ok_owned(uint256 v) external pure returns (uint256) {
        uint256[] memory a = new uint256[](2);
        a[1] = v;
        uint256 w = a[1];
        return w;
    }
    /// @custom:ensures result == v
    function ok_param_entry(uint256[] memory a, uint256 v) public pure returns (uint256) {
        if (a.length == 0) return v;
        a[0] = v;
        return a[0];
    }
}
