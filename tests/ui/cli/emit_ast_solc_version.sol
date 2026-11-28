//@ revisions: 0_8 0_9
//@[0_8] compile-flags: --emit=ast --pretty-json
//@[0_9] compile-flags: --emit=ast --pretty-json --solc-version 0.9.0
//@[0_8] filecheck: --check-prefix=V08
//@[0_9] filecheck: --check-prefix=V09
//@ normalize-stdout-test: "(?s).+" -> ""

// V08-LABEL: "nodeType": "ModifierDefinition"
// V08: "virtual": false
// V08: "isConstant": false
// V08: "version": "0.8.39+
// V09-LABEL: "nodeType": "ModifierDefinition"
// V09-NOT: "virtual"
// V09: "body"
// V09-NOT: "isConstant"
// V09: "version": "0.9.0+
contract C {
    modifier m() {
        _;
    }

    function f() public pure m returns (uint256) {
        return 1;
    }
}
