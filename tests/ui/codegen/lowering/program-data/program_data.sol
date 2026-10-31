//@ revisions: mir size runtime paris
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck:
//@[size] compile-flags: -Osize -Zdump=mir,evm-ir-runtime
//@[size] filecheck: --check-prefixes=SIZE,SPLAT
//@[runtime] compile-flags: -Ogas
//@[paris] compile-flags: -Osize --evm-version paris
//@ run-call: dataHash => 0x8319c67c70fe039f53a0eeaf4cf710931f907cb64c36ddad3dd91983363828d9
//@ run-call: subsliceHash => 0xc35006aafafcb6b8331ca170fbfc98c11825034a545300a1d8392d4d36902026
//@ run-call: zeroHash => 0xdfded4ed5ac76ba7379cfe7b3b0f53e768dca8d45a34854e649cfc3c18cbd9cd
//@ run-call: zeroWordHash => 0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563
//@ run-call: splatHash => 0x779ba5798a6ad3d608b17a14735a3a2d7d61e8c9817435fc4524dd5d0cf6a177

// CHECK-LABEL: @data
// CHECK: literal_0: hex"
// CHECK: literal_1: hex"
// CHECK: literal_2: hex"
// CHECK-NOT: literal_3:
contract C {
    // CHECK-LABEL: fn @data(
    // CHECK: datacopy literal_0, {{.*}}, 288
    // SIZE-LABEL: fn @data(
    // SIZE: mstore {{.*}}, 0
    // SIZE: datacopy literal_0, {{.*}}, 257
    function data() external pure returns (bytes memory) {
        return "sztswcbwub6sj95y2psa0x432cf1tkscxeiv05thqz5gkewi146p6io410dd3uopq4chl5jl0q78f603tvbzcswc8qo7pqd0hj1dbp629dsg0lio445df0nnt8u8ob9jefcazcou5q3662wgfpgqvixb48euz4hdd14hqsf32binx85n8slx821umj214u4jk844syn28lm5u4q1v0oo04557gqlotstZy5xw1j5u1e7vvg0nj9cxalsvfkpgcrc!";
    }

    // CHECK-LABEL: fn @subslice(
    // CHECK: datacopy literal_1, {{.*}}, 192
    // SIZE-LABEL: fn @subslice(
    // SIZE: mstore {{.*}}, 0
    // SIZE: datacopy literal_1, {{.*}}, 161
    function subslice() external pure returns (bytes memory) {
        return "q4chl5jl0q78f603tvbzcswc8qo7pqd0hj1dbp629dsg0lio445df0nnt8u8ob9jefcazcou5q3662wgfpgqvixb48euz4hdd14hqsf32binx85n8slx821umj214u4jk844syn28lm5u4q1v0oo04557gqlotstZ";
    }

    function dataHash() external pure returns (bytes32) {
        return keccak256("sztswcbwub6sj95y2psa0x432cf1tkscxeiv05thqz5gkewi146p6io410dd3uopq4chl5jl0q78f603tvbzcswc8qo7pqd0hj1dbp629dsg0lio445df0nnt8u8ob9jefcazcou5q3662wgfpgqvixb48euz4hdd14hqsf32binx85n8slx821umj214u4jk844syn28lm5u4q1v0oo04557gqlotstZy5xw1j5u1e7vvg0nj9cxalsvfkpgcrc!");
    }

    function subsliceHash() external pure returns (bytes32) {
        return keccak256("q4chl5jl0q78f603tvbzcswc8qo7pqd0hj1dbp629dsg0lio445df0nnt8u8ob9jefcazcou5q3662wgfpgqvixb48euz4hdd14hqsf32binx85n8slx821umj214u4jk844syn28lm5u4q1v0oo04557gqlotstZ");
    }

    // CHECK-LABEL: fn @zeroData{{[.(]}}
    // CHECK: memory_zero {{.*}}, 160
    function zeroData() public pure returns (bytes memory) {
        return hex"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
    }

    // CHECK-LABEL: fn @zeroWord{{[.(]}}
    // CHECK: mstore {{.*}}, 0
    function zeroWord() public pure returns (bytes memory) {
        return hex"0000000000000000000000000000000000000000000000000000000000000000";
    }

    // CHECK-LABEL: fn @splatData{{[.(]}}
    // CHECK: datacopy literal_2, {{.*}}, 160
    // SPLAT-LABEL: fn @splatData.body() -> i256
    // SPLAT-COUNT-2: mstore {{.*}}, 0x112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00
    // SPLAT: mcopy {{.*}}, 64
    // SPLAT: mcopy {{.*}}, 32
    function splatData() public pure returns (bytes memory) {
        return hex"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00";
    }

    function zeroHash() external pure returns (bytes32) {
        return keccak256(zeroData());
    }

    function zeroWordHash() external pure returns (bytes32) {
        return keccak256(zeroWord());
    }

    function splatHash() external pure returns (bytes32) {
        return keccak256(splatData());
    }
}

// SIZE-LABEL: (runtime) ===
// SIZE: push_data literal_0
