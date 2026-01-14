//@compile-flags: -Ztypeck

// ========== UDVT explicit cast tests ==========

type MyUint is uint256;
type MyAddress is address;
type MyInt is int128;

contract UDVTTests {
    // Valid: UDVT explicit cast (underlying -> UDVT)
    function testWrap(uint256 x, address a, int128 i) public pure {
        MyUint u = MyUint(x);
        MyAddress ma = MyAddress(a);
        MyInt mi = MyInt(i);
    }
    
    // Valid: UDVT explicit cast (UDVT -> underlying)
    function testUnwrap(MyUint u, MyAddress ma, MyInt mi) public pure {
        uint256 x = uint256(u);
        address a = address(ma);
        int128 i = int128(mi);
    }
    
    // Valid: round-trip conversion
    function testRoundTrip(uint256 x) public pure returns (uint256) {
        MyUint u = MyUint(x);
        return uint256(u);
    }
}

// ========== bytes <-> string tests ==========

contract BytesStringTests {
    // Valid: string memory -> bytes memory (via unlocated cast)
    function testStringToBytes(string memory s) public pure returns (bytes memory) {
        return bytes(s);
    }
    
    // Valid: bytes memory -> string memory (via unlocated cast)
    function testBytesToString(bytes memory b) public pure returns (string memory) {
        return string(b);
    }
    
    // Valid: string calldata -> bytes calldata
    function testCalldataStringToBytes(string calldata s) external pure returns (bytes calldata) {
        return bytes(s);
    }
    
    // Valid: bytes calldata -> string calldata
    function testCalldataBytesToString(bytes calldata b) external pure returns (string calldata) {
        return string(b);
    }
    
    // Valid: Same location Ref -> Ref conversion
    function testRefToRef(string memory s, bytes memory b) public pure {
        bytes memory b1 = bytes(s);
        string memory s1 = string(b);
    }
}

// ========== Function type conversions ==========

contract FunctionTypeTests {
    function externalPure(uint256 x) external pure returns (uint256) { return x; }
    function externalView(uint256 x) external view returns (uint256) { return x; }
    
    // Valid: function pointer with same signature
    function testFunctionPointerConversion() public view {
        function(uint256) external pure returns (uint256) f1 = this.externalPure;
        function(uint256) external view returns (uint256) f2 = this.externalView;
    }
}

// ========== Tuple explicit conversions ==========

contract TupleTests {
    // Valid: tuple element-wise explicit conversions
    function testTupleConversion() public pure {
        (int8 a, int16 b) = (int8(1), int16(2));
        (int256 c, int256 d) = (int256(a), int256(b));
    }
    
    // Valid: nested tuple conversion
    function testNestedTypes(uint8 a, uint16 b, uint32 c) public pure {
        (uint256 x, uint256 y, uint256 z) = (uint256(a), uint256(b), uint256(c));
    }
}

// ========== Error cases ==========

contract ExplicitConversionErrors {
    // Invalid: cannot convert bytes memory to string calldata (location mismatch)
    function testLocationMismatch(bytes memory b) external pure {
        string calldata s = string(b); //~ERROR: mismatched types
    }
    
    // Invalid: cannot convert uint256 to string
    function testInvalidToString(uint256 x) public pure {
        string memory s = string(x); //~ERROR: invalid explicit type conversion
    }
    
    // Invalid: cannot convert string to uint256
    function testInvalidFromString(string memory s) public pure {
        uint256 x = uint256(s); //~ERROR: invalid explicit type conversion
    }
    
    // Invalid: mismatched tuple lengths
    function testTupleLengthMismatch() public pure {
        (int8 a, int16 b, int32 c) = (int8(1), int16(2), int32(3));
        (int256 d, int256 e) = (int256(a), int256(b), int256(c)); //~ERROR: mismatched number of components
    }
}

// ========== UDVT Error cases ==========

type Price is uint256;
type Amount is uint128;

contract UDVTErrors {
    // Invalid: cannot convert UDVT to incompatible underlying type directly
    function testWrongUnderlying(Price p) public pure {
        // This should fail: Price is uint256, cannot cast directly to int256
        int256 x = int256(p); //~ERROR: invalid explicit type conversion
    }
    
    // Invalid: cannot convert one UDVT to another UDVT
    function testUDVTtoUDVT(MyUint u) public pure {
        Price p = Price(u); //~ERROR: invalid explicit type conversion
    }
    
    // Invalid: cannot convert UDVT to different size underlying
    function testUDVTtoWrongSize(Price p) public pure {
        uint128 x = uint128(p); //~ERROR: invalid explicit type conversion
    }
}

// ========== Function type error cases ==========

contract FunctionTypeErrors {
    function extFn(uint256 x) external pure returns (uint256) { return x; }
    function extFnDiffParams(uint256 x, uint256 y) external pure returns (uint256) { return x + y; }
    function extFnDiffReturn(uint256 x) external pure returns (int256) { return int256(x); }
    
    // Valid assignments (implicit) - just for reference
    function testValidAssignment() public view {
        function(uint256) external pure returns (uint256) f1 = this.extFn;
    }
}
