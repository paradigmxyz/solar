// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/179_assignment_of_overloaded_function.sol
contract test {
    function f() public returns (uint) { return 1; }
    function f(uint a) public returns (uint) { return 2 * a; }
    function g() public returns (uint) { function (uint) returns (uint) x = f; return x(7); } //~ ERROR: no matching declarations found
}
