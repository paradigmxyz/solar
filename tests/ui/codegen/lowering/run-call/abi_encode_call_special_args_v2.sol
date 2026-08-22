//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: test() => true
//@[gas] run-call: test() => true
//@[size] run-call: test() => true
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_call_special_args_v2.sol

contract AbiEncodeCallSpecialArgs {
    function fNoArgs() external {}
    function fArray(uint[] memory) external {}
    function fUint(uint, uint) external returns (uint, uint) {}

    function test() external view returns (bool) {
        uint[] memory values;
        return keccak256(abi.encodeWithSignature("fNoArgs()"))
                == keccak256(abi.encodeCall(this.fNoArgs, ()))
            && keccak256(abi.encodeWithSignature("fArray(uint256[])", values))
                == keccak256(abi.encodeCall(this.fArray, values))
            && keccak256(abi.encodeWithSignature("fUint(uint256,uint256)", 12, 13))
                == keccak256(abi.encodeCall(this.fUint, (12, 13)));
    }
}
