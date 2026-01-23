// SPDX-License-Identifier: MIT
pragma solidity >=0.8.10;

import {Test} from "forge-std/Test.sol";

library MathLib {
    function min(uint256 a, uint256 b) internal pure returns (uint256) {
        return a < b ? a : b;
    }

    function sqrt(uint256 x) internal pure returns (uint256 z) {
        assembly {
            z := 181
            let r := shl(7, lt(0xffffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0x3ffffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0x1fffff, shr(r, x))))
            z := shl(shr(1, r), z)
            z := shr(18, mul(z, add(shr(r, x), 65536)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := sub(z, lt(div(x, z), z))
        }
    }
}

contract LibraryUser {
    using MathLib for uint256;

    uint256 public result;

    function computeMin(uint256 a, uint256 b) public {
        result = a.min(b);
    }

    function computeSqrt(uint256 x) public {
        result = x.sqrt();
    }

    function computeComplex(uint256 a, uint256 b) public returns (uint256) {
        uint256 product = a * b;
        uint256 sqrtVal = product.sqrt();
        return sqrtVal.min(1000);
    }
}

contract LibraryUsingTest is Test {
    LibraryUser lib;

    function setUp() public {
        lib = new LibraryUser();
    }

    function testMin() public {
        lib.computeMin(10, 20);
        assertEq(lib.result(), 10);
    }

    function testSqrt() public {
        lib.computeSqrt(100);
        assertEq(lib.result(), 10);
    }

    function testComplex() public {
        uint256 result = lib.computeComplex(25, 16);
        assertEq(result, 20);
    }
}
