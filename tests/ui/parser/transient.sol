contract C {
    uint256 transient;

    function f() external {
        transient = 1;
    }
}

contract D {
    uint256 transient transient;

    function f() external {
        transient = 2;
    }
}

contract E {
    function f(uint256 transient) external { 
        transient = 3;
    }
    
    function f2(uint256 transient, bool) external { 
        transient = 4;
    }

    function g(uint256 transient transient) external { //~ ERROR: data location can only be specified for array, struct or mapping types
        transient = 5;
    }

    function g2(uint256[] transient transient) external { //~ ERROR: invalid data location
        transient = 5;
    }

  }
