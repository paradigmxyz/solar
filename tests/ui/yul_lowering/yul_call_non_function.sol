//@compile-flags: -Ztypeck

contract C {
    function helper(uint256 a) internal pure returns (uint256) {
        return a;
    }

    function f(uint256 dialect_helper) public {
        assembly {
            dialect_helper(1) //~ ERROR: expected function
        }
    }

    function g() public {
        assembly {
            helper(1) //~ ERROR: unresolved symbol
        }
    }
}
