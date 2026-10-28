library L {
    function f(uint256 x) external pure returns (uint256) {
        return x + 1;
    }
}

contract Child {
    uint256 public y = 7;
}

contract C {
    uint256 immutable z;

    constructor() {
        z = block.number;
    }

    function g(uint256 x) external view returns (uint256) {
        return L.f(x) + z;
    }

    function make() external returns (address) {
        return address(new Child());
    }

    function code() external pure returns (bytes memory) {
        return type(Child).runtimeCode;
    }
}
