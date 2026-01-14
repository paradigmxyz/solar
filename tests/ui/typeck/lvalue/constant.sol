//@compile-flags: -Ztypeck

contract Test {
    uint256 constant CONST = 1;

    function test() external { //~ WARN: function state mutability can be restricted to pure
        CONST = 2; //~ ERROR: cannot assign to a constant variable
    }
}

uint256 constant FILE_CONST = 1;

function fileLevel() { //~ WARN: function state mutability can be restricted to pure
    FILE_CONST = 2; //~ ERROR: cannot assign to a constant variable
}
