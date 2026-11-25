// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract A2 {
    struct Q { uint256[] xs; }
    Q internal q;
    uint256[] internal st;

    function _id(uint256[] memory a) internal pure returns (uint256[] memory) { return a; }

    /// A memory local named like the state variable q: assigning a into it is not a copy to storage.
    /// @custom:ensures result == 0
    function bad_shadow_state() external pure returns (uint256 result) {
        uint256[] memory a = new uint256[](1);
        Q memory q = Q(new uint256[](1));
        q.xs = a;
        q.xs[0] = 5;
        result = a[0];
    }

    // Refused: the analysis is over names, and q is a memory local here.
    /// @custom:ensures result == 5
    // expect: write through memory reference `q`, which may share its memory
    function bad_shadow_state_true_spec() external pure returns (uint256 result) {
        uint256[] memory a = new uint256[](1);
        Q memory q = Q(new uint256[](1));
        q.xs = a;
        q.xs[0] = 5;
        result = q.xs[0];
    }

    /// Writing through a call result: _id(a) is a.
    /// @custom:ensures result == 0
    function bad_call_result_write() external pure returns (uint256 result) {
        uint256[] memory a = new uint256[](1);
        _id(a)[0] = 5;
        result = a[0];
    }

    /// A memory local named like state st, initialised from another memory array.
    /// @custom:ensures result == 0
    function bad_shadow_decl() external pure returns (uint256 result) {
        uint256[] memory a = new uint256[](1);
        uint256[] memory st = a;
        st[0] = 5;
        result = a[0];
    }
}
