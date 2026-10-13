// ported-from: test/libsolidity/syntaxTests/constants/initialization/bytes_concat_non_const_args.sol
contract A {
    function getData() public view returns (bytes memory) {
        return msg.data;
    }

    function getDataPure() public pure returns (bytes memory) {
        return hex"ffff";
    }

    bytes constant abData = bytes.concat(hex"aaaa", hex"bbbb", msg.data); //~ ERROR: initial value for constant variable has to be compile-time constant
    bytes constant abgetData = bytes.concat(hex"aaaa", hex"bbbb", getData()); //~ ERROR: initial value for constant variable has to be compile-time constant
    bytes constant abgetDataPure = bytes.concat(hex"aaaa", hex"bbbb", getDataPure()); //~ ERROR: initial value for constant variable has to be compile-time constant
}
