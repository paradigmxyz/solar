// Struct members are sized one by one, and a struct array element counts one word per member.
contract C {
    struct S {
        uint256[67108864] a;
        uint256[67108864] b;
    }
    struct T {
        uint256[134217728] a;
    }
    struct U {
        T t;
        uint256 x;
    }
    struct Many {
        uint256 a;
        uint256 b;
        uint256 c;
        uint256 d;
    }

    function decode(bytes memory data) public pure {
        abi.decode(data, (S));
        abi.decode(data, (S[2]));
        abi.decode(data, (T[3]));
        abi.decode(data, (T)); //~ ERROR: type too large for memory
        abi.decode(data, (U)); //~ ERROR: type too large for memory
    }

    function mem() public pure {
        S memory s;
        T[3] memory t;
        Many[33554431] memory m;
        U memory u; //~ ERROR: type too large for memory
        Many[33554432] memory n; //~ ERROR: type too large for memory
        s; t; m; u; n;
    }

    function cd(S calldata s, Many[8388608] calldata m) external pure {
        s; m;
    }

    function cdTooLarge(Many[33554432] calldata m) external pure { //~ ERROR: type too large for calldata
        m;
    }
}
