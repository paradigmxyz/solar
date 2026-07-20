//@compile-flags: -Zcodegen -Zdump=mir

contract MappingBytesValues {
    mapping(uint256 => bytes) data;
    mapping(uint256 => mapping(uint256 => string)) nested;

    function set(uint256 key, bytes memory value) external {
        data[key] = value;
    }

    function setNested(uint256 outer, uint256 inner, string memory value) external {
        nested[outer][inner] = value;
    }

    function get(uint256 key) external view returns (bytes memory) {
        return data[key];
    }

    function getNested(uint256 outer, uint256 inner) external view returns (string memory) {
        return nested[outer][inner];
    }
}
