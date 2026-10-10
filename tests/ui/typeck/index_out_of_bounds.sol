// ported-from: test/libsolidity/syntaxTests/indexing/array_out_of_bounds_index.sol
// ported-from: test/libsolidity/syntaxTests/indexing/fixedbytes_out_of_bounds_index.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/231_array_out_of_bound_access.sol

contract C {
  function f() public {
    bytes[32] memory a;
    a[64]; //~ ERROR: out of bounds array access
  }
}

contract D {
  function f() public {
    bytes32 b;
    b[64]; //~ ERROR: out of bounds array access
  }
}

contract c {
    uint[2] dataArray;
    function set5th() public returns (bool) {
        dataArray[5] = 2; //~ ERROR: out of bounds array access
        return true;
    }
}

contract E {
    uint[2] s;
    uint[2][3] s2;
    uint constant K = 5;

    function f(uint[3] calldata cd, uint[] calldata dyn) external {
        uint[4] memory m;
        bytes4 b;
        bytes memory bs;

        s[1];
        s[2]; //~ ERROR: out of bounds array access
        s2[2][1];
        s2[3][0]; //~ ERROR: out of bounds array access
        s2[1][2]; //~ ERROR: out of bounds array access
        cd[2];
        cd[3]; //~ ERROR: out of bounds array access
        m[1 + 3]; //~ ERROR: out of bounds array access
        m[0x4]; //~ ERROR: out of bounds array access
        m[4e0]; //~ ERROR: out of bounds array access
        [1, 2][2]; //~ ERROR: out of bounds array access
        delete s[2]; //~ ERROR: out of bounds array access
        b[3];
        b[4]; //~ ERROR: out of bounds array access

        // Only literal indexes are checked.
        m[K];
        m[uint(4)];
        m[true ? 4 : 5];
        dyn[100];
        bs[100];
    }
}
