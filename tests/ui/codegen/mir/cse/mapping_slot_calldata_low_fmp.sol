//@ codegen-matrix: standard
//@ run-call: f 0x00000000000000000000000000000000000000000000000000000000000012340000000000000000000000000000000000000000000000000000000000001000 => 0x0000000000000000000000000000000000000000000000000000000000001234

// Assembly that moves the free memory pointer into scratch makes the first
// calldata-key hash overwrite the pointer, so the second hash writes elsewhere
// and must stay.
contract MappingSlotCalldataLowFmp {
    mapping(bytes => uint256) m;

    function f(bytes calldata b) external returns (uint256 r) {
        assembly {
            mstore(0x40, 0x20)
        }
        m[b] = 7;
        r = m[b];
        assembly {
            return(0x1000, 0x20)
        }
    }
}
