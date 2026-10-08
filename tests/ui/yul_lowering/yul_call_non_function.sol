contract C {
    function helper(uint256 a) internal pure returns (uint256) {
        return a;
    }

    function f(uint256 dialect_helper) public {
        assembly {
            dialect_helper(1) //~ ERROR: unresolved symbol
        }
    }

    function g() public {
        assembly {
            helper(1) //~ ERROR: unresolved symbol
        }
    }

    function h() public {
        function(uint256) internal pure returns (uint256) local_helper = helper;
        assembly {
            local_helper(1) //~ ERROR: unresolved symbol
        }
    }

    function i() public {
        assembly {
            let yul_variable := 1
            yul_variable(1) //~ ERROR: expected function, found variable
        }
    }
}
