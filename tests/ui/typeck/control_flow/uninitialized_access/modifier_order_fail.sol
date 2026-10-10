// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/modifier_order_fail.sol
contract C {
    modifier m1(uint[] storage a) { _; }
    modifier m2(uint[] storage a) { _; }
    uint[] s;
    function f() m1(b) m2(b = s) internal view returns (uint[] storage b) {} //~ ERROR: this variable is of storage pointer type and can be accessed
}
