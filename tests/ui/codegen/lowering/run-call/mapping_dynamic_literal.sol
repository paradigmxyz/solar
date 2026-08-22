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
//@[none] run-call: readCalldata(string) "abc" => 7
//@[gas] run-call: readCalldata(string) "abc" => 7
//@[size] run-call: readCalldata(string) "abc" => 7
//@[none] run-call: readMemory(string) "abc" => 7
//@[gas] run-call: readMemory(string) "abc" => 7
//@[size] run-call: readMemory(string) "abc" => 7
//@[none] run-call: callReadMemory(string) "abc" => 7
//@[gas] run-call: callReadMemory(string) "abc" => 7
//@[size] run-call: callReadMemory(string) "abc" => 7
//@[none] run-call: readStorage() => 7
//@[gas] run-call: readStorage() => 7
//@[size] run-call: readStorage() => 7
//@[none] run-call: readLongLiteral() => 11
//@[gas] run-call: readLongLiteral() => 11
//@[size] run-call: readLongLiteral() => 11
//@[none] run-call: overwriteShort() => 13
//@[gas] run-call: overwriteShort() => 13
//@[size] run-call: overwriteShort() => 13

contract MappingDynamicLiteral {
    string private key;
    mapping(string => uint256) private values;

    constructor() {
        key = "abc";
        values["abc"] = 7;
        values["a literal key longer than thirty-two bytes, hashed in full"] = 11;
    }

    function readCalldata(string calldata query) external view returns (uint256) {
        return values[query];
    }

    function readMemory(string memory query) external view returns (uint256) {
        return values[query];
    }

    function readMemoryPublic(string memory query) public view returns (uint256) {
        return values[query];
    }

    function callReadMemory(string memory query) external view returns (uint256) {
        return readMemoryPublic(query);
    }

    function readStorage() external view returns (uint256) {
        return values[key];
    }

    function readLongLiteral() external view returns (uint256) {
        return values["a literal key longer than thirty-two bytes, hashed in full"];
    }

    function overwriteShort() external returns (uint256) {
        values["abc"] = 13;
        return values[key];
    }
}
