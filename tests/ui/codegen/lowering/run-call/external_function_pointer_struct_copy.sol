//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: test() => true
// ported-from: test/libsolidity/semanticTests/structs/function_type_copy.sol

pragma abicoder v2;

contract ExternalFunctionPointerStructCopyTarget {
    struct Holder {
        function() external[] functions;
    }

    function copy(function() external[] calldata functions)
        external
        pure
        returns (Holder memory)
    {
        Holder memory holder;
        holder.functions = functions;
        return holder;
    }
}

contract ExternalFunctionPointerStructCopy {
    ExternalFunctionPointerStructCopyTarget private target;

    constructor() {
        target = new ExternalFunctionPointerStructCopyTarget();
    }

    function test() external view returns (bool) {
        function() external[] memory functions =
            new function() external[](3);
        functions[0] = this.random1;
        functions[1] = this.random2;
        functions[2] = this.random3;

        ExternalFunctionPointerStructCopyTarget.Holder memory result = target.copy(functions);
        require(result.functions.length == 3);
        require(result.functions[0] == this.random1);
        require(result.functions[1] == this.random2);
        require(result.functions[2] == this.random3);
        return true;
    }

    function random1() external pure {}
    function random2() external pure {}
    function random3() external pure {}
}
