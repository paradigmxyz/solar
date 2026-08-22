//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: test() => 11116
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_call_declaration_v2.sol

contract AbiEncodeCallDeclarationTarget {
    function a(uint) public pure {}
    function b(uint) external pure {}
}

contract AbiEncodeCallDeclarationBase {
    function a(uint value) external pure returns (uint) {
        return value + 1;
    }
}

contract AbiEncodeCallDeclaration is AbiEncodeCallDeclarationBase {
    function test() public view returns (uint result) {
        bool success;
        bytes memory data;

        (success, data) = address(this).staticcall(
            abi.encodeCall(AbiEncodeCallDeclarationTarget.a, 1)
        );
        require(success && data.length == 32);
        result += abi.decode(data, (uint));
        require(result == 2);

        (success, data) = address(this).staticcall(
            abi.encodeCall(AbiEncodeCallDeclarationTarget.b, 10)
        );
        require(success && data.length == 32);
        result += abi.decode(data, (uint));
        require(result == 13);

        (success, data) = address(this).staticcall(
            abi.encodeCall(AbiEncodeCallDeclarationBase.a, 100)
        );
        require(success && data.length == 32);
        result += abi.decode(data, (uint));
        require(result == 114);

        (success, data) = address(this).staticcall(abi.encodeCall(this.a, 1000));
        require(success && data.length == 32);
        result += abi.decode(data, (uint));
        require(result == 1115);

        (success, data) = address(this).staticcall(
            abi.encodeCall(AbiEncodeCallDeclaration.b, 10000)
        );
        require(success && data.length == 32);
        result += abi.decode(data, (uint));
        require(result == 11116);

        return result;
    }

    function b(uint value) external view returns (uint) {
        return this.a(value);
    }
}
