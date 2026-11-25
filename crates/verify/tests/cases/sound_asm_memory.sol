// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract A1 {
    struct P { uint256 a; uint256 b; }

    // fmp-relative write aliases constant offset 0x80 (fmp == 0x80 when nothing allocated)
    function bad_fmp_alias() public pure returns (uint256 a, uint256 b) {
        assembly {
            let p := mload(0x40)
            a := mload(0x80)
            mstore(p, add(a, 1))
            b := mload(0x80)
        }
        assert(a == b);
    }

    function ok_fmp_alias() public pure returns (uint256 a, uint256 b) {
        assembly {
            a := mload(0x20)
            b := mload(0x20)
        }
        assert(a == b);
    }

    // memory struct pointer value is a fake constant
    function bad_struct_ptr_value() public pure returns (uint256 x) {
        P memory s = P(1, 2);
        assembly { x := s }
        assert(x > 0xffffffff);
    }

    function ok_struct_ptr_field() public pure returns (uint256 x) {
        P memory s = P(1, 2);
        assembly { x := mload(add(s, 0x20)) }
        assert(x == 2);
    }

    // struct pointer is beyond the free memory pointer in the model
    function bad_struct_vs_fmp() public pure returns (uint256 x, uint256 f) {
        P memory s = P(1, 2);
        assembly { x := s f := mload(0x40) }
        assert(x >= f);
    }

    // two distinct structs in two blocks get the same pointer
    function bad_struct_two_blocks() public pure returns (uint256 x, uint256 y) {
        P memory s = P(1, 2);
        P memory t = P(3, 4);
        assembly { x := s }
        assembly { y := t }
        assert(x == y);
    }

    // aliased struct in one block: distinct pointers
    function bad_struct_alias() public pure returns (uint256 x, uint256 y) {
        P memory s = P(1, 2);
        P memory t = s;
        assembly { x := s y := t }
        assert(x != y);
    }
}
