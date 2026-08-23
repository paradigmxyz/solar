//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: raw() => 0x0000000000000000000000000000000000000000000000000000000000003412
//@ run-call: packed 171, 52719 => 0x0000000000000000000000000000000000000000000000000000000000cdefab

contract PackedUint {
    uint8 public a = 0x12;
    uint16 public b = 0x34;

    function raw() external view returns (bytes32 value) {
        assembly {
            value := sload(0)
        }
    }

    function packed(uint8 x, uint16 y) external returns (bytes32 value) {
        a = x;
        b = y;
        assembly {
            value := sload(0)
        }
    }
}
