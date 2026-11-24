//@ revisions: auto linear
//@[auto] compile-flags: -O gas -Zdump=mir-final
//@[auto] filecheck: --check-prefix=AUTO
//@[linear] compile-flags: -O gas -Zdump=mir-final -Zswitch-lowering=linear
//@[linear] filecheck: --check-prefix=LINEAR
//@ normalize-stdout-test: "(?s).+" -> ""

// A forced switch lowering leaves the switch to the backend.
contract ForcedLowering {
    // AUTO-LABEL: fn @pick(
    // AUTO-NOT: switch
    // AUTO: = shr
    // LINEAR-LABEL: fn @pick(
    // LINEAR: switch {{.*}}, [3 => bb{{[0-9]+}}, 5 =>
    function pick(uint256 i) external pure returns (uint256 v) {
        assembly {
            switch i
            case 3 { v := 300 }
            case 5 { v := 500 }
            case 9 { v := 900 }
            default { v := 7 }
        }
    }
}
