// ported-from: test/libsolidity/syntaxTests/inlineAssembly/external_identifier_access_shadowing.sol

contract C {
    uint s;
    uint[] arr;
    uint constant K = 1;

    function f() public returns (uint x) {
        assembly {
            function g() -> x {
                x := 42 //~ ERROR: cannot access local Solidity variables from inside an inline assembly function
            }
            x := g()
        }
    }

    function argument(uint p) public pure {
        assembly {
            function g() -> r {
                function h() -> q {
                    q := p //~ ERROR: cannot access local Solidity variables from inside an inline assembly function
                }
                r := h()
            }
            pop(g())
            pop(p)
        }
    }

    function storagePointer() public view {
        uint[] storage ptr = arr;
        assembly {
            function g() -> r {
                r := ptr.slot //~ ERROR: cannot access local Solidity variables from inside an inline assembly function
            }
            pop(g())
        }
    }

    function allowed() public view returns (uint later) {
        assembly {
            function g() -> r {
                r := add(s.slot, K)
                let inner := 1
                r := add(r, inner)
            }
            later := g()
        }
    }
}
