//@ revisions: osaka byzantium
//@[osaka] compile-flags: -O gas -Zdump=mir-final
//@[osaka] filecheck: --check-prefix=OSAKA
//@[byzantium] compile-flags: -O gas -Zdump=mir-final --evm-version byzantium
//@[byzantium] filecheck: --check-prefix=BYZANTIUM
//@ normalize-stdout-test: "(?s).+" -> ""

// Before Constantinople, shifts lower to `EXP` and `DIV`, which the lookup's price does not
// cover, so the switch stays.
contract PreConstantinople {
    // OSAKA-LABEL: fn @pick(
    // OSAKA-NOT: switch
    // OSAKA: = shr
    // BYZANTIUM-LABEL: fn @pick(
    // BYZANTIUM: switch {{.*}}, [3 => bb{{[0-9]+}}, 5 =>
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
