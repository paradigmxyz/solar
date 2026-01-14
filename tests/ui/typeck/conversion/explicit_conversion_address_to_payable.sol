//@compile-flags: -Ztypeck
contract C {
    function g(address payable _p) internal pure returns (uint) {
        return 1;
    }
    function f(address _a) public pure {
        uint x = g(payable(_a)); //~ ERROR: not yet implemented
        uint y = g(_a); //~ ERROR: not yet implemented
    }
}
