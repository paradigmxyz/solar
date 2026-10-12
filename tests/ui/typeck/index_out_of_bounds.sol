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
