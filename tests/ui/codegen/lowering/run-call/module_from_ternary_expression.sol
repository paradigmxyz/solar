//@ codegen-matrix: standard
//@ run-call: f => true
// ported-from: test/libsolidity/semanticTests/expressions/module_from_ternary_expression.sol

import "./auxiliary/module_from_ternary_expression.sol" as M;

contract C {
    function f() public pure returns (bool) {
        bool flag;
        ((flag = true) ? M : M).D;
        return flag;
    }
}
