//@compile-flags: -Ztypeck

contract C {
    function f() public returns (uint256 x) {
        assembly {
            function one(a, b) -> r {
                r := add(a, b)
            }

            x := one(1)
        }
    }

    function g(uint256 dialect_helper) public {
        assembly {
            dialect_helper(1)
        }
    }
}
