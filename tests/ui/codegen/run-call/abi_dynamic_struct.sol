//@ run-call: init((address,uint8,string,bytes),address) (0x000000000000000000000000000000000000beef, 7, "name", 0x0102), 0x0000000000000000000000000000000000000003 => 10
//@ run-call: tail((address,uint8,string,bytes)) (0x000000000000000000000000000000000000beef, 7, "name", 0x0102) => 0x02

struct InitInput {
    address asset;
    uint8 decimals;
    string name;
    bytes params;
}

contract AbiDynamicStruct {
    function init(InitInput calldata input, address sink) external pure returns (uint256) {
        return input.decimals + uint160(sink);
    }

    function tail(InitInput calldata input) external pure returns (bytes memory) {
        return input.params[1:];
    }
}
