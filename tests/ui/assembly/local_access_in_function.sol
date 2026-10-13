contract C {
    uint s;
    uint constant K = 1;

    function argument(uint p) public pure {
        assembly {
            function g() -> r {
                r := p //~ ERROR: cannot access local Solidity variables from inside an inline assembly function
            }
            pop(g())
        }
    }

    function shadowedConstant() public pure {
        uint K = 2;
        assembly {
            function g() -> r {
                r := K //~ ERROR: cannot access local Solidity variables from inside an inline assembly function
            }
            pop(g())
        }
    }

    function allowed() public view returns (uint later) {
        assembly {
            function g() -> r {
                r := add(s.slot, K)
            }
            later := g()
        }
    }

    function hiddenFunctionTypeName(function(uint K) external cb) public pure returns (uint r) { //~ WARN: named function type parameters are deprecated
        assembly {
            function g() -> x {
                x := K
            }
            r := g()
        }
    }
}
