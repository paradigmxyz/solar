//@ revisions: homestead byzantium
//@[homestead] compile-flags: --evm-version homestead
//@[byzantium] compile-flags: --evm-version byzantium
// ported-from: test/libsolidity/syntaxTests/abiEncoder/v2_accessing_returned_dynamic_array_without_returndata_support.sol
// ported-from: test/libsolidity/syntaxTests/abiEncoder/v2_accessing_returned_dynamic_array_with_returndata_support.sol

pragma abicoder v2;

contract C {
    function get() public view returns (uint[][] memory) {}

    function test() public view returns (bool) {
        uint[][] memory x = this.get();
        //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
    }
}
