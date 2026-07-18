//@compile-flags: -Zcodegen --emit=mir

contract StorageBytesPushPop {
    bytes data;

    constructor() {
        data.push(0x01);
        data.push(0x02);
    }

    function pushValue(bytes1 value) external {
        data.push(value);
    }

    function pushZero() external {
        data.push();
    }

    function popValue() external {
        data.pop();
    }

    function get() external view returns (bytes memory) {
        return data;
    }
}
