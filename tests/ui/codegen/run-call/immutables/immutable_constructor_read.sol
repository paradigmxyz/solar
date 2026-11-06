//@ revisions: default gas size
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: ConstructorReads::narrow => 255, 5, 170
//@ run-call: ConstructorReads::signed => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe
//@ run-call: ConstructorReads::word => 0x1234
//@ run-call: ConstructorReads::account => 0x000000000000000000000000000000000000beef
//@ run-call: ConstructorReads::flag => 1
//@ run-call: ConstructorReads::helper => 0x2a
//@ run-call: ConstructorReads::sharedHelper => 0x2a
//@ run-call: ConstructorReads::sharedTerminal => 7
//@ run-call: ConstructorReads::sharedTerminalLoop => 81

contract Narrow {
    uint8 immutable a = 0xff;
    uint8 immutable b = 5;
    bytes1 immutable c = 0xaa;

    constructor() {
        uint8 x = a;
        uint8 y = b;
        bytes1 z = c;
        assembly {
            mstore(0, x)
            mstore(32, y)
            mstore(64, shr(248, z))
            return(0, 96)
        }
    }
}

contract Signed {
    int8 immutable a = -2;

    constructor() {
        int8 x = a;
        assembly {
            mstore(0, signextend(0, x))
            return(0, 32)
        }
    }
}

contract Word {
    uint256 immutable a = 0x1234;

    constructor() {
        uint256 x = a;
        assembly {
            mstore(0, x)
            return(0, 32)
        }
    }
}

contract Account {
    address immutable a = address(0xbeef);

    constructor() {
        address x = a;
        assembly {
            mstore(0, x)
            return(0, 32)
        }
    }
}

contract Flag {
    bool immutable a = true;

    constructor() {
        bool x = a;
        assembly {
            mstore(0, x)
            return(0, 32)
        }
    }
}

contract Helper {
    uint256 immutable a;

    constructor() {
        a = 0x2a;
        uint256 x = read();
        assembly {
            mstore(0, x)
            return(0, 32)
        }
    }

    function read() internal view returns (uint256) {
        return a;
    }
}

contract SharedHelper {
    uint256 immutable a;
    uint256 public total;

    constructor() {
        a = 0x2a;
        uint256 x = sum();
        assembly {
            mstore(0, x)
            return(0, 32)
        }
    }

    function sum() internal view returns (uint256 r) {
        r = a;
        for (uint256 i; i < total; i++) r += i * a;
    }

    function runtime() external view returns (uint256) {
        return sum();
    }
}

contract SharedTerminal {
    uint256 immutable a;

    constructor(uint256 v) {
        a = v;
        finish();
    }

    function finish() internal view {
        uint256 v = a;
        assembly {
            mstore(0, v)
            return(0, 32)
        }
    }

    function runtime() external view {
        finish();
    }
}

contract SharedTerminalLoop {
    uint256 immutable a;
    uint256 public total;

    constructor(uint256 v) {
        a = v;
        finish(v);
    }

    function finish(uint256 n) internal view {
        uint256 r;
        for (uint256 i; i < n; i++) r += a + total;
        assembly {
            mstore(0, r)
            return(0, 32)
        }
    }

    function runtime(uint256 n) external view {
        finish(n);
    }
}

contract ConstructorReads {
    function narrow() external returns (uint256, uint256, uint256) {
        bytes memory code = address(new Narrow()).code;
        return abi.decode(code, (uint256, uint256, uint256));
    }

    function signed() external returns (uint256) {
        return uint256(bytes32(address(new Signed()).code));
    }

    function word() external returns (uint256) {
        return uint256(bytes32(address(new Word()).code));
    }

    function account() external returns (address) {
        return address(uint160(uint256(bytes32(address(new Account()).code))));
    }

    function flag() external returns (uint256) {
        return uint256(bytes32(address(new Flag()).code));
    }

    function helper() external returns (uint256) {
        return uint256(bytes32(address(new Helper()).code));
    }

    function sharedHelper() external returns (uint256) {
        return uint256(bytes32(address(new SharedHelper()).code));
    }

    function sharedTerminal() external returns (uint256) {
        return uint256(bytes32(address(new SharedTerminal(7)).code));
    }

    function sharedTerminalLoop() external returns (uint256) {
        return uint256(bytes32(address(new SharedTerminalLoop(9)).code));
    }
}
