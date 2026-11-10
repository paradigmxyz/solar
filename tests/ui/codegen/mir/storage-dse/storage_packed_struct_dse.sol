//@ codegen-matrix: standard
//@ run-call: PackedStructDse::clearsLonePadding => true

// The field owns its slot, so the store keeps no neighbors. See CODEGEN-025 in
// docs/SOLC_DIVERGENCE.md.
contract PackedStructDse {
    struct Box {
        uint8 value;
    }

    Box internal box;

    function clearsLonePadding() public returns (bool) {
        assembly {
            sstore(box.slot, not(0))
        }
        box = Box({value: 1});

        uint256 raw;
        assembly {
            raw := sload(box.slot)
        }
        return raw == 1;
    }
}
