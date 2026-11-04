library Math {
    struct Tuple {
        uint256 a;
    }
}

contract Token {
    uint256 public constant BPS = 10_000;
    uint256 public stored;

    function viaThis() external view returns (uint256) {
        return this.BPS();
    }
}

contract User {
    Token token;

    function viaInstance() external view returns (uint256) {
        return token.BPS();
    }

    function notConstant() external view returns (uint256) {
        return token.stored();
    }

    function decodeLibraryType() external pure {
        abi.decode("", (Math.Tuple));
    }

    function decodeElementaryType() external pure {
        abi.decode("", (uint256));
    }
}
