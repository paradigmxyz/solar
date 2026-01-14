//@compile-flags: -Ztypeck
// Pure functions cannot read state variables.
// https://github.com/paradigmxyz/solar/issues/221

contract C {
    uint256 stateVar = 10;
    uint256 constant CONST = 42;
    
    // Error: pure function reads state.
    function testPure() public pure returns (uint256) {
        return stateVar;
        //~^ ERROR: function declared as pure, but this expression reads
    }
    
    // OK: view function can read state.
    function testView() public view returns (uint256) {
        return stateVar;
    }
    
    // OK: pure function can read constants.
    function testPureConst() public pure returns (uint256) {
        return CONST;
    }
}
