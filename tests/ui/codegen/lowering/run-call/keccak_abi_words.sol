//@ codegen-matrix: standard
//@ run-call: words 7, 13 => true
//@ run-call: mixed -2, 0x61626364, true => true
//@ run-call: dirty => true
//@ run-call: ordered => true

contract KeccakAbiWords {
    uint256 counter;

    function words(uint256 a, uint256 b) external pure returns (bool) {
        return keccak256(abi.encode(a, b)) == keccak256(abi.encodePacked(a, b));
    }

    function mixed(int8 a, bytes4 b, bool flag) public pure returns (bool) {
        return keccak256(abi.encode(a, b))
                == keccak256(abi.encodePacked(int256(a), bytes32(b)))
            && keccak256(abi.encode(flag)) == keccak256(abi.encodePacked(uint256(flag ? 1 : 0)));
    }

    function dirty() external pure returns (bool) {
        int8 a;
        bytes4 b;
        bool flag;
        assembly {
            a := 254
            b := not(0)
            flag := 2
        }
        return mixed(a, b, flag);
    }

    function next() private returns (uint256) {
        return ++counter;
    }

    function ordered() external returns (bool) {
        return keccak256(abi.encode(next(), next()))
            == keccak256(abi.encodePacked(uint256(1), uint256(2))) && counter == 2;
    }
}
