//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract StorageBytesElements {
    bytes public b;

    function init(bytes memory value) public {
        b = value;
    }

    function poke() public {
        b[5] = 0xAA;
    }

    function hashB() public view returns (bytes32) {
        return keccak256(b);
    }
}

contract StorageStringConstructor {
    string public name;
    string public symbol;

    constructor(string memory name_, string memory symbol_) {
        name = name_;
        symbol = symbol_;
    }
}

contract StorageStringBase {
    string public name;
    string public symbol;

    constructor(string memory name_, string memory symbol_) {
        name = name_;
        symbol = symbol_;
    }
}

contract StorageStringDerived is StorageStringBase {
    constructor() StorageStringBase("ERC20Mock", "E20M") {}
}

contract StorageStringImplicitDerived is StorageStringBase("Base Literal Name", "BLN") {}
