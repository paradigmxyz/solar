// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// A function-pointer local that shadows a builtin (sha256) is taken for the builtin by the memory
// alias check, which then treats the array argument as copied rather than shared.
contract PtrShadowAlias {
    function id(uint256[] memory a) internal pure returns (uint256[] memory) {
        return a;
    }

    /// b is a, so b[0] reads the 5 written through a: result == 5, never 0.
    /// @custom:ensures result == 0
    function bad_shadow(uint256 x) public pure returns (uint256 result) {
        function(uint256[] memory) internal pure returns (uint256[] memory) sha256 = id;
        uint256[] memory a = new uint256[](1);
        uint256[] memory b = sha256(a);
        a[0] = 5;
        result = b[0] + x - x;
    }

    /// The same code with the pointer under a plain name.
    /// @custom:ensures result == 0
    function plain_name(uint256 x) public pure returns (uint256 result) {
        function(uint256[] memory) internal pure returns (uint256[] memory) h = id;
        uint256[] memory a = new uint256[](1);
        uint256[] memory b = h(a);
        a[0] = 5;
        result = b[0] + x - x;
    }

    // Refused: the pointer may hold a function that keeps a reference to its argument.
    /// @custom:ensures result == 0
    // expect: memory argument `a` of `id` may share its memory with another reference
    function bad_shadow_no_write(uint256 x) public pure returns (uint256 result) {
        function(uint256[] memory) internal pure returns (uint256[] memory) sha256 = id;
        uint256[] memory a = new uint256[](1);
        uint256[] memory b = sha256(a);
        result = b[0] + x - x;
    }
}
