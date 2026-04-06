//@compile-flags: -Ztypeck

contract Test {
    uint256 constant CONST = 1;

    function test() external {
        CONST = 2; //~ ERROR: cannot assign to a constant variable
    }
}

uint256 constant FILE_CONST = 1;

function fileLevel() {
    FILE_CONST = 2; //~ ERROR: cannot assign to a constant variable
}
