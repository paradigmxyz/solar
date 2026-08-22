//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: raw() => 0x0000000000000000000000000000000000000000000000000000000000003412
//@[gas] run-call: raw() => 0x0000000000000000000000000000000000000000000000000000000000003412
//@[size] run-call: raw() => 0x0000000000000000000000000000000000000000000000000000000000003412
//@[none] run-call: packed 171, 52719 => 0x0000000000000000000000000000000000000000000000000000000000cdefab
//@[gas] run-call: packed 171, 52719 => 0x0000000000000000000000000000000000000000000000000000000000cdefab
//@[size] run-call: packed 171, 52719 => 0x0000000000000000000000000000000000000000000000000000000000cdefab

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
