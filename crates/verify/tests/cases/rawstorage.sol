pragma solidity ^0.8.20;
/// Raw storage that assembly reaches is modelled apart from the declared state variables, so it
/// must stay clear of their slots; a packed struct written through its slot must hold valid
/// bool and enum bytes.
contract RawStorage {
    uint256 public x;
    mapping(uint256 => uint256) m;
    enum E { A, B, C }
    struct S { E e; bool b; uint8 y; }
    S s;

    /// @custom:ensures x == old(x)
    function bad_overlap() external {
        assembly { sstore(0, 7) }
    }
    /// @custom:ensures x == old(x)
    function bad_overlap_param(uint256 k) external {
        assembly { sstore(k, 7) }
    }
    /// @custom:ensures x == old(x)
    function ok_far_slot() external {
        assembly { sstore(0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc, 7) }
    }
    /// Slots are counted without packing, an upper bound: `s.slot` is 2, but 2 to 4 are refused.
    /// @custom:ensures x == old(x)
    function ok_past_layout(uint256 k) external {
        require(k >= 5);
        assembly { sstore(k, 7) }
    }
    /// @custom:ensures uint8(s.e) == 2
    function bad_enum_byte() external {
        S storage p = s;
        assembly { sstore(p.slot, 5) }
    }
    /// @custom:ensures result == 0x070101
    function bad_bool_byte() external returns (uint256 result) {
        S storage p = s;
        assembly { sstore(p.slot, 0x0201) }
        p.y = 7;
        assembly { result := sload(p.slot) }
    }
    /// @custom:ensures result == 0x070101
    function ok_valid_bytes() external returns (uint256 result) {
        S storage p = s;
        assembly { sstore(p.slot, 0x0101) }
        p.y = 7;
        assembly { result := sload(p.slot) }
    }
}
