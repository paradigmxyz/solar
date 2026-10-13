// ported-from: test/libyul/yulSyntaxTests/declaration_duplicate_vars.yul
// ported-from: test/libyul/yulSyntaxTests/for_visibility_E.yul
// ported-from: test/libyul/yulSyntaxTests/function_shadowing_outside_vars_1.yul
// ported-from: test/libyul/yulSyntaxTests/name_clashes.yul
// ported-from: test/libyul/yulSyntaxTests/vardecl_multi_conflict.yul
// ported-from: test/libyul/yulSyntaxTests/vardecl_name_clashes.yul

contract C {
    function functions() public {
        assembly {
            function g() {}
            function g() {} //~ ERROR: function name `g` already taken
        }
        assembly {
            function h() {}
            function h(a) -> b {} //~ ERROR: function name `h` already taken
        }
        assembly {
            function k() {}
            let k := 1 //~ ERROR: variable name `k` already taken
        }
        assembly {
            let g := 2 //~ ERROR: variable name `g` already taken
            function g() {}
        }
    }

    function variables() public {
        assembly {
            let x := 1
            let x := 2 //~ ERROR: variable name `x` already taken
        }
        assembly {
            let a, a := 0 //~ ERROR: variable name `a` already taken
        }
        assembly {
            function f() -> a, b {}
            let x, x := f() //~ ERROR: variable name `x` already taken
        }
    }

    function parameters() public {
        assembly {
            function p(a, a) {} //~ ERROR: variable name `a` already taken
        }
        assembly {
            function q(b) -> b {} //~ ERROR: variable name `b` already taken
        }
        assembly {
            function r() -> c, c {} //~ ERROR: variable name `c` already taken
        }
        assembly {
            { let x }
            function f() -> x {}
        }
        assembly {
            for {} 1 { let x := 1 } { let x := 1 }
        }
    }

    // NAME-001: unlike solc, Yul declarations can shadow names from enclosing Yul scopes.
    function shadowing() public {
        assembly {
            let x := 1
            { let x := 2 }
        }
        assembly {
            function g() {
                function g() {}
            }
        }
        assembly {
            { function g() {} }
            function g() {}
        }
        assembly {
            function g() {
                let g := 0
            }
        }
        assembly {
            { let g := 0 }
            function g() {}
        }
        assembly {
            function g(h) {
                function h() {}
            }
        }
        assembly {
            function g() {}
            function f(g) {}
        }
        assembly {
            let x
            function f() -> x {}
        }
        assembly {
            for { let x := 1 } 1 { let x := 1 } {}
        }
        assembly {
            let x := 1
            for {} 1 {} { let x := 1 }
        }
    }
}
