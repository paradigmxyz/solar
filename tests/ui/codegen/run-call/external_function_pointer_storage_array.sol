//@ run-call: runAll() => "[a called][b called][a called][b called][a called][b called]"
// ported-from: test/libsolidity/semanticTests/array/copying/function_type_array_to_storage.sol

contract ExternalFunctionPointerStorageArray {
    string log;
    function() external[] fs;
    function() external[] gs;

    function a() external {
        log = string.concat(log, "[a called]");
    }

    function b() external {
        log = string.concat(log, "[b called]");
    }

    function storeCalldata(function() external[] calldata pointers) external {
        fs = pointers;
    }

    function runAll() external returns (string memory) {
        function() external[] memory pointers = new function() external[](2);
        pointers[0] = this.a;
        pointers[1] = this.b;
        fs = pointers;
        fs[0]();
        fs[1]();
        this.storeCalldata(pointers);
        fs[0]();
        fs[1]();
        gs = fs;
        gs[0]();
        gs[1]();
        return log;
    }
}
