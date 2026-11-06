//@ codegen-matrix: standard
//@ run-call: callField 4 => 8
//@ run-call: callField 0 => 1
//@ run-call: callReturnedField => 7

// A `try` target can be an external function pointer read from a struct
// field, including a field of a struct returned by another call.
contract TryMemberFunctionPointer {
    struct Callback {
        function(uint256) external returns (uint256) run;
    }

    struct File {
        function() external view returns (uint256) read;
    }

    function double(uint256 value) external pure returns (uint256) {
        require(value != 0, "zero");
        return value * 2;
    }

    function seven() external pure returns (uint256) {
        return 7;
    }

    function file() external view returns (File memory) {
        return File(this.seven);
    }

    function callField(uint256 value) external returns (uint256) {
        Callback memory callback = Callback(this.double);
        try callback.run(value) returns (uint256 result) {
            return result;
        } catch {
            return 1;
        }
    }

    function callReturnedField() external view returns (uint256) {
        try this.file().read() returns (uint256 result) {
            return result;
        } catch {
            return 0;
        }
    }
}
