//@ run-call: readCalldata(string) "abc" => 7
//@ run-call: readStorage() => 7
//@ run-call: readLongLiteral() => 11
//@ run-call: overwriteShort() => 13

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
