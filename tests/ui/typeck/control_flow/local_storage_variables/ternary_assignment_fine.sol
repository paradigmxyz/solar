// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/ternary_assignment_fine.sol
contract C {
    uint256[] s;
    function f() public view {
        uint256[] storage x;
        uint256[] storage y = (x = s)[0] > 0 ? x : x;
        y;
    }
}
