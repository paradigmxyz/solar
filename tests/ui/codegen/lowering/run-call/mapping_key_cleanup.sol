//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test_cleanup => true
// ported-from: test/libsolidity/semanticTests/viaYul/storage/mappings.sol

contract C {
    mapping(uint16 => uint) cleanup;

    function test_cleanup() public returns (bool) {
        uint16 x;
        assembly {
            x := 0xffff0001
        }
        cleanup[x] = 3;
        return cleanup[1] == 3;
    }
}
