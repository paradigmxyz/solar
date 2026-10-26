import "./auxiliary/module_conditional.sol" as M;

contract C {
    function f(bool c) public pure returns (uint256) {
        return (c ? M : M).x; //~ ERROR: invalid true type
        //~^ ERROR: invalid false type
    }
}
