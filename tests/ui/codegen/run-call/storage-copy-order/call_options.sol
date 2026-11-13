//@ codegen-matrix: standard
//@ run-call: gasOption => 9
//@ run-call: newPlain => 9
//@ run-call: newSalt => 9
// A call with options is a writing operand like one without, so a storage array before it is
// copied after it ran, as solc does via IR.
contract D {
    constructor() {
        Test(msg.sender).poke();
    }
}

contract Test {
    uint256[] sa;

    constructor() {
        sa.push(1);
    }

    function poke() external returns (uint256) {
        sa[0] = 9;
        return 0;
    }

    function first(uint256[] memory m, uint256 z) internal pure returns (uint256) {
        return m[0] + z;
    }

    function gasOption() external returns (uint256) {
        return first(sa, this.poke{gas: 100000}());
    }

    function newPlain() external returns (uint256) {
        return first(sa, address(new D()) == address(0) ? 1 : 0);
    }

    function newSalt() external returns (uint256) {
        return first(sa, address(new D{salt: bytes32(0)}()) == address(0) ? 1 : 0);
    }
}
