//@ codegen-matrix: standard portable gasmir sizemir
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[gasmir] compile-flags: -Ogas -Zdump=mir
//@[sizemir] compile-flags: -Osize -Zdump=mir
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[gasmir] normalize-stdout-test: "(?s).+" -> ""
//@[sizemir] normalize-stdout-test: "(?s).+" -> ""
//@[mir] filecheck:
//@[gasmir] filecheck: --check-prefix=GAS
//@[sizemir] filecheck: --check-prefix=SIZE
//@[gas] run-call: gasFirst => true
//@[portable] run-call: gasFirst => true
//@[none] run-call: gasFirst => false
//@[size] run-call: gasFirst => false
//@ run-call: firstZero 0x0061000000000000000000000000000000000000000000000000000000000000 => 0
//@ run-call: firstZero 0x6162630000000000000000000000000000000000000000000000000000000000 => 3
//@ run-call: firstZero 0x6161616161616161616161616161616161616161616161616161616161616161 => 32

import {Build} from "solar:core/v1/Build.sol";

contract Test {
    // The body answers true for other compilers; this one answers whether the
    // build optimizes for gas.
    // CHECK-LABEL: fn @gasFirst{{.*}}selector=
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: zext i1 0 to i256
    function gasFirst() public pure returns (bool) {
        return Build.gasFirst();
    }

    // The index of the first zero byte of `s`, or 32. Gas builds test the
    // first byte by itself first; the loop gives the same answer, and size
    // builds keep only the loop.
    // GAS-LABEL: fn @firstZero{{[( ]}}
    // GAS: byte 0,
    // SIZE-LABEL: fn @firstZero{{[( ]}}
    // SIZE-NOT: byte 0,
    // SIZE: {{^}$}}
    function firstZero(bytes32 s) public pure returns (uint256 i) {
        if (Build.gasFirst() && s[0] == 0) return 0;
        while (i < 32 && s[i] != 0) ++i;
    }
}
