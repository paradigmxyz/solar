//@ codegen-matrix: standard
//@ compile-flags: --evm-version spuriousDragon
//@ run-call: viewCall => 9
//@ run-call: pureCall => 9
// Before Byzantium an external `view` or `pure` call is a plain `CALL`, so the callee can write
// storage and a storage array before it is copied after it ran.
interface Peek {
    function peek() external view returns (uint256);
    function pick() external pure returns (uint256);
}

contract Evil {
    function peek() external returns (uint256) {
        Test(msg.sender).poke();
        return 0;
    }

    function pick() external returns (uint256) {
        Test(msg.sender).poke();
        return 0;
    }
}

contract Test {
    uint256[] sa;

    constructor() {
        sa.push(1);
    }

    function poke() external {
        sa[0] = 9;
    }

    function first(uint256[] memory m, uint256 z) internal pure returns (uint256) {
        return m[0] + z;
    }

    function viewCall() external returns (uint256) {
        Peek evil = Peek(address(new Evil()));
        return first(sa, evil.peek());
    }

    function pureCall() external returns (uint256) {
        Peek evil = Peek(address(new Evil()));
        return first(sa, evil.pick());
    }
}
