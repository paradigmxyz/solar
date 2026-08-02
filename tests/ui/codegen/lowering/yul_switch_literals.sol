//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract YulSwitchLiterals {
    // CHECK-LABEL: fn @selectorLiteral
    // CHECK: switch 0x3100000000000000000000000000000000000000000000000000000000000000
    function selectorLiteral() external pure {
        assembly {
            switch "1"
            case "1" {}
            default {}
        }
    }

    // CHECK-LABEL: fn @caseLiterals
    // CHECK: switch arg0, default {{bb[0-9]+}}, [0x3100000000000000000000000000000000000000000000000000000000000000 => {{bb[0-9]+}}, 0x1122000000000000000000000000000000000000000000000000000000000000 => {{bb[0-9]+}}]
    function caseLiterals(uint256 value) external pure {
        assembly {
            switch value
            case "1" {}
            case hex"1122" {}
            default {}
        }
    }
}
