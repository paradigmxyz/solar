//@ codegen-matrix: standard
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: literal => 0
//@ run-call: constantInput => 64
//@ run-call: stringConversion => 64
//@ run-call: bytesConversion => 64
//@ run-call: functionPointer => 64
//@ run-call: tryCall => 64
//@ run-call: named => 64
//@ run-call: namedReversed => 0
//@ run-call: pointerLiteral => 0

contract LiteralInputArgumentOrder {
    string constant VALUE = "hello";

    function pointer() internal pure returns (uint256 p) {
        assembly { p := mload(64) }
    }

    function sink(string calldata input, uint256 observed, uint256 beforePointer)
        external pure returns (uint256)
    {
        require(keccak256(bytes(input)) == keccak256("hello"));
        return observed - beforePointer;
    }

    function literal() external view returns (uint256) {
        uint256 beforePointer = pointer();
        return this.sink("hello", pointer(), beforePointer);
    }

    function constantInput() external view returns (uint256) {
        uint256 beforePointer = pointer();
        return this.sink(VALUE, pointer(), beforePointer);
    }

    function stringConversion() external view returns (uint256) {
        uint256 beforePointer = pointer();
        return this.sink(string("hello"), pointer(), beforePointer);
    }

    function bytesConversion() external view returns (uint256) {
        uint256 beforePointer = pointer();
        return this.sink(string(bytes("hello")), pointer(), beforePointer);
    }

    function functionPointer() external view returns (uint256) {
        function(string memory, uint256, uint256) external view returns (uint256) callback = this.sink;
        uint256 beforePointer = pointer();
        return callback(VALUE, pointer(), beforePointer);
    }

    function tryCall() external view returns (uint256) {
        uint256 beforePointer = pointer();
        try this.sink(VALUE, pointer(), beforePointer) returns (uint256 delta) {
            return delta;
        } catch {
            revert();
        }
    }

    function named() external view returns (uint256) {
        uint256 beforePointer = pointer();
        return this.sink({input: VALUE, observed: pointer(), beforePointer: beforePointer});
    }

    function namedReversed() external view returns (uint256) {
        uint256 beforePointer = pointer();
        return this.sink({observed: pointer(), input: VALUE, beforePointer: beforePointer});
    }

    function pointerLiteral() external view returns (uint256) {
        function(string memory, uint256, uint256) external view returns (uint256) callback = this.sink;
        uint256 beforePointer = pointer();
        return callback("hello", pointer(), beforePointer);
    }
}
