// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/ternary_assignment_err.sol
contract C {
    uint256[] s;
    function f() public {
        bool d;
        uint256[] storage x;
        uint256[] storage y = d ? (x = s) : x; //~ ERROR: this variable is of storage pointer type and can be accessed
        y;
    }
}
