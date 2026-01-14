//@compile-flags: -Ztypeck
// Power operations with integer literals that overflow should be rejected.
// https://github.com/paradigmxyz/solar/issues/222

contract C {
    function test() public pure returns (uint256) {
        uint256 result = 10 ** 1e10;
        //~^ ERROR: built-in binary operator `**` cannot be applied
        return result;
    }
    
    function test2() public pure returns (uint256) {
        uint256 result = 2 ** 256;
        //~^ ERROR: built-in binary operator `**` cannot be applied
        return result;
    }
    
    // Valid cases - should compile.
    function testOk() public pure returns (uint256) {
        uint256 a = 2 ** 255;
        uint256 b = 10 ** 77;
        return a + b;
    }
}
