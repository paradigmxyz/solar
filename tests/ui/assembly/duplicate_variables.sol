contract C {
    function f() public pure {
        assembly {
            let x := 1
            let x := 2 //~ ERROR: identifier `x` already declared
        }
    }

    function g() public pure {
        assembly {
            let a, a //~ ERROR: identifier `a` already declared
        }
    }

    function h() public pure {
        assembly {
            function p(a, a) {} //~ ERROR: identifier `a` already declared
            function q(b) -> b {} //~ ERROR: identifier `b` already declared
        }
    }

    function i() public pure {
        assembly {
            function k() {}
            let k := 1 //~ ERROR: identifier `k` already declared
        }
    }

    function j() public pure {
        assembly {
            let l := 1 //~ ERROR: identifier `l` already declared
            function l() {}
        }
    }
}
