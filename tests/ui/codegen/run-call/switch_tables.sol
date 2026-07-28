//@ revisions: auto buckets perfect
//@[buckets] compile-flags: -Zswitch-lowering=buckets
//@[perfect] compile-flags: -Zswitch-lowering=perfect
//@ run-call: testBucketDispatch()
//@ run-call: testBucketDispatchMiss()
//@ run-call: testDenseSwitch()
//@ run-call: testSparseInternalSwitch()
//@ run-call: testBinarySwitch()
//@ run-call: testAffineSwitch()

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SwitchTables {
    uint256 public value;

    function dense(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 10 { result := 100 }
            case 11 { result := 101 }
            case 12 { result := 102 }
            case 13 { result := 103 }
            case 14 { result := 104 }
            case 15 { result := 105 }
            case 16 { result := 106 }
            case 17 { result := 107 }
            case 18 { result := 108 }
            case 19 { result := 109 }
            case 20 { result := 110 }
            case 21 { result := 111 }
            case 23 { result := 113 }
            case 24 { result := 114 }
            case 25 { result := 115 }
            case 26 { result := 116 }
            case 27 { result := 117 }
            case 28 { result := 118 }
            case 29 { result := 119 }
            case 30 { result := 120 }
            case 31 { result := 121 }
            case 32 { result := 122 }
            case 33 { result := 123 }
            default { result := 999 }
        }
    }

    function sparse(uint256 key) external pure returns (uint256) {
        return sparseInternal(key, 1);
    }

    function sparseInternal(uint256 key, uint256 depth) internal pure returns (uint256 result) {
        assembly {
            switch key
            case 0xcbf99d38 { result := 200 }
            case 0x87d912cb { result := 201 }
            case 0x920f5c73 { result := 202 }
            case 0x41052a0d { result := 203 }
            case 0x7238232f { result := 204 }
            case 0x905f7d67 { result := 205 }
            case 0x3b88f6c2 { result := 206 }
            case 0xaa66aa63 { result := 207 }
            case 0x24a75cfd { result := 208 }
            case 0x98e9a73d { result := 209 }
            case 0xebd25c8f { result := 210 }
            case 0x8a67ee70 { result := 211 }
            case 0xc43b1a78 { result := 212 }
            case 0xebbd40a9 { result := 213 }
            case 0x1eb6457a { result := 214 }
            case 0xdca2fb5a { result := 215 }
            case 0x67e648b5 { result := 216 }
            case 0xe6e0ae36 { result := 217 }
            case 0xcba58af7 { result := 218 }
            case 0x6d4975a2 { result := 219 }
            case 0x8dc714ba { result := 220 }
            case 0x54eaadab { result := 221 }
            case 0x10c772cc { result := 222 }
            case 0x4e580bc4 { result := 223 }
            case 0x965a68f5 { result := 224 }
            case 0xd758c88e { result := 225 }
            case 0x6d738a50 { result := 226 }
            case 0xf02a00c9 { result := 227 }
            case 0x40bcff2a { result := 228 }
            case 0x5fb43592 { result := 229 }
            case 0x3d492ec4 { result := 230 }
            case 0x1f49dbe7 { result := 231 }
            default { result := 999 }
        }
        if (depth != 0) {
            return sparseInternal(key, depth - 1);
        }
    }

    function f00() external {
        value = 0;
    }

    function f01() external {
        value = 1;
    }

    function f02() external {
        value = 2;
    }

    function f03() external {
        value = 3;
    }

    function f04() external {
        value = 4;
    }

    function f05() external {
        value = 5;
    }

    function f06() external {
        value = 6;
    }

    function f07() external {
        value = 7;
    }

    function f08() external {
        value = 8;
    }

    function f09() external {
        value = 9;
    }

    function f10() external {
        value = 10;
    }

    function f11() external {
        value = 11;
    }

    function f12() external {
        value = 12;
    }

    function f13() external {
        value = 13;
    }

    function f14() external {
        value = 14;
    }

    function f15() external {
        value = 15;
    }

    function f16() external {
        value = 16;
    }

    function f17() external {
        value = 17;
    }

    function f18() external {
        value = 18;
    }

    function f19() external {
        value = 19;
    }

    function f20() external {
        value = 20;
    }

    function f21() external {
        value = 21;
    }

    function f22() external {
        value = 22;
    }

    function f23() external {
        value = 23;
    }

    function f24() external {
        value = 24;
    }

    function f25() external {
        value = 25;
    }

    function f26() external {
        value = 26;
    }

    function f27() external {
        value = 27;
    }

    function f28() external {
        value = 28;
    }

    function f29() external {
        value = 29;
    }

    function f30() external {
        value = 30;
    }

    function f31() external {
        value = 31;
    }
}

contract BinarySwitch {
    function select(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 1 { result := 300 }
            case 7920 { result := 301 }
            case 15839 { result := 302 }
            case 23758 { result := 303 }
            case 31677 { result := 304 }
            case 39596 { result := 305 }
            case 47515 { result := 306 }
            default { result := 999 }
        }
    }

    function selectAffine(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 20000 { result := 400 }
            case 20006 { result := 401 }
            case 20012 { result := 402 }
            case 20018 { result := 403 }
            case 20024 { result := 404 }
            case 20030 { result := 405 }
            case 20036 { result := 406 }
            case 20042 { result := 407 }
            case 20048 { result := 408 }
            case 20054 { result := 409 }
            case 20060 { result := 410 }
            case 20066 { result := 411 }
            case 20072 { result := 412 }
            case 20078 { result := 413 }
            case 20084 { result := 414 }
            case 20090 { result := 415 }
            case 20096 { result := 416 }
            case 20102 { result := 417 }
            case 20108 { result := 418 }
            case 20114 { result := 419 }
            case 20120 { result := 420 }
            case 20126 { result := 421 }
            case 20132 { result := 422 }
            case 20138 { result := 423 }
            case 20144 { result := 424 }
            case 20150 { result := 425 }
            case 20156 { result := 426 }
            case 20162 { result := 427 }
            case 20168 { result := 428 }
            case 20174 { result := 429 }
            case 20180 { result := 430 }
            case 20186 { result := 431 }
            case 20192 { result := 432 }
            case 20198 { result := 433 }
            case 20204 { result := 434 }
            case 20210 { result := 435 }
            case 20216 { result := 436 }
            case 20222 { result := 437 }
            case 20228 { result := 438 }
            case 20234 { result := 439 }
            case 20240 { result := 440 }
            case 20246 { result := 441 }
            case 20252 { result := 442 }
            case 20258 { result := 443 }
            case 20264 { result := 444 }
            case 20270 { result := 445 }
            case 20276 { result := 446 }
            case 20282 { result := 447 }
            case 20288 { result := 448 }
            case 20294 { result := 449 }
            case 20300 { result := 450 }
            case 20306 { result := 451 }
            case 20312 { result := 452 }
            case 20318 { result := 453 }
            case 20324 { result := 454 }
            case 20330 { result := 455 }
            case 20336 { result := 456 }
            case 20342 { result := 457 }
            case 20348 { result := 458 }
            case 20354 { result := 459 }
            case 20360 { result := 460 }
            case 20366 { result := 461 }
            case 20372 { result := 462 }
            case 20378 { result := 463 }
            case 20384 { result := 464 }
            default { result := 999 }
        }
    }
}

contract SwitchTablesTest {
    SwitchTables tables;
    BinarySwitch binarySwitch;

    function setUp() public {
        tables = new SwitchTables();
        binarySwitch = new BinarySwitch();
    }

    function testBucketDispatch() public {
        tables.f00();
        assert(tables.value() == 0);
        tables.f01();
        assert(tables.value() == 1);
        tables.f02();
        assert(tables.value() == 2);
        tables.f03();
        assert(tables.value() == 3);
        tables.f04();
        assert(tables.value() == 4);
        tables.f05();
        assert(tables.value() == 5);
        tables.f06();
        assert(tables.value() == 6);
        tables.f07();
        assert(tables.value() == 7);
        tables.f08();
        assert(tables.value() == 8);
        tables.f09();
        assert(tables.value() == 9);
        tables.f10();
        assert(tables.value() == 10);
        tables.f11();
        assert(tables.value() == 11);
        tables.f12();
        assert(tables.value() == 12);
        tables.f13();
        assert(tables.value() == 13);
        tables.f14();
        assert(tables.value() == 14);
        tables.f15();
        assert(tables.value() == 15);
        tables.f16();
        assert(tables.value() == 16);
        tables.f17();
        assert(tables.value() == 17);
        tables.f18();
        assert(tables.value() == 18);
        tables.f19();
        assert(tables.value() == 19);
        tables.f20();
        assert(tables.value() == 20);
        tables.f21();
        assert(tables.value() == 21);
        tables.f22();
        assert(tables.value() == 22);
        tables.f23();
        assert(tables.value() == 23);
        tables.f24();
        assert(tables.value() == 24);
        tables.f25();
        assert(tables.value() == 25);
        tables.f26();
        assert(tables.value() == 26);
        tables.f27();
        assert(tables.value() == 27);
        tables.f28();
        assert(tables.value() == 28);
        tables.f29();
        assert(tables.value() == 29);
        tables.f30();
        assert(tables.value() == 30);
        tables.f31();
        assert(tables.value() == 31);
    }

    function testBucketDispatchMiss() public {
        (bool firstBucket,) = address(tables).call(hex"00000000");
        assert(!firstBucket);
        (bool lastBucket,) = address(tables).call(hex"00000026");
        assert(!lastBucket);
        (bool interiorBucket,) = address(tables).call(hex"ffffffff");
        assert(!interiorBucket);
    }

    function testDenseSwitch() public view {
        assert(tables.dense(9) == 999);
        assert(tables.dense(10) == 100);
        assert(tables.dense(21) == 111);
        assert(tables.dense(22) == 999);
        assert(tables.dense(33) == 123);
        assert(tables.dense(34) == 999);
    }

    function testSparseInternalSwitch() public view {
        assert(tables.sparse(0xcbf99d38) == 200);
        assert(tables.sparse(0x67e648b5) == 216);
        assert(tables.sparse(0x1f49dbe7) == 231);
        assert(tables.sparse(1) == 999);
    }

    function testBinarySwitch() public view {
        assert(binarySwitch.select(0) == 999);
        assert(binarySwitch.select(1) == 300);
        assert(binarySwitch.select(7920) == 301);
        assert(binarySwitch.select(15839) == 302);
        assert(binarySwitch.select(23758) == 303);
        assert(binarySwitch.select(31677) == 304);
        assert(binarySwitch.select(39596) == 305);
        assert(binarySwitch.select(47515) == 306);
        assert(binarySwitch.select(47516) == 999);
    }

    function testAffineSwitch() public view {
        assert(binarySwitch.selectAffine(19999) == 999);
        assert(binarySwitch.selectAffine(20000) == 400);
        assert(binarySwitch.selectAffine(20001) == 999);
        assert(binarySwitch.selectAffine(20192) == 432);
        assert(binarySwitch.selectAffine(20384) == 464);
        assert(binarySwitch.selectAffine(20385) == 999);
    }
}
