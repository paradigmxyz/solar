//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: run() => 7
// ported-from: test/libsolidity/semanticTests/functionTypes/internal_function_pointer_with_calldata_args.sol

contract InternalFunctionPointerCalldata {
    function(bytes calldata) internal returns (bytes1) target;

    constructor() {
        target = read;
    }

    function read(bytes calldata data) internal pure returns (bytes1) {
        return data[2];
    }

    function invoke(bytes calldata data) external returns (bytes1) {
        return target(data);
    }

    function run() external returns (uint8) {
        bytes memory data = new bytes(34);
        data[2] = bytes1(uint8(7));
        return uint8(this.invoke(data));
    }
}
