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

contract Written {
    uint256 a;
    uint256 b;

    function swap() external {
        (a, b) = (b, a);
    }

    function clear() external pure {
        delete ([""][0]);
    }
}

contract Slices {
    function indexSlice(bytes calldata b, uint256[] calldata values) external pure {
        // Indexing a calldata slice is not an lvalue, nor is indexing calldata.
        b[1:3][0];
        values[1:][1:][0];
        b[0];
        values[0];
    }

    function indexMemory(bytes memory b, uint256[] memory values) external pure {
        b[0];
        values[0];
    }
}
