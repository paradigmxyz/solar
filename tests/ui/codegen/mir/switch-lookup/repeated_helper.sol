//@ compile-flags: -O gas -Zdump=mir-final
//@ filecheck:

// A helper with a 40-key switch of 40-bit literals needs a seven-word lookup, which costs more gas
// than the switch's jump table. Inlining runs after the pass, so the helper's switch runs once per
// iteration of its caller's loop and stays a switch. Called once, it becomes the lookup, whose
// deposit saving repays its gas.
contract RepeatedHelper {
    // CHECK-LABEL: RepeatedHelper ===
    // CHECK: switch {{.*}}, [0 => bb{{[0-9]+}}, 1 =>
    function sum(uint256 n) external pure returns (uint256 s) {
        assembly {
            function pick(i) -> v {
                switch i
                case 0 { v := 0x89e3779b97 }
                case 1 { v := 0x93c6ef372e }
                case 2 { v := 0x9daa66d2c5 }
                case 3 { v := 0xa78dde6e5c }
                case 4 { v := 0xb1715609f3 }
                case 5 { v := 0xbb54cda58a }
                case 6 { v := 0xc538454121 }
                case 7 { v := 0xcf1bbcdcb8 }
                case 8 { v := 0xd8ff34784f }
                case 9 { v := 0xe2e2ac13e6 }
                case 10 { v := 0xecc623af7d }
                case 11 { v := 0xf6a99b4b14 }
                case 12 { v := 0x808d12e6ab }
                case 13 { v := 0x8a708a8242 }
                case 14 { v := 0x9454021dd9 }
                case 15 { v := 0x9e3779b970 }
                case 16 { v := 0xa81af15507 }
                case 17 { v := 0xb1fe68f09e }
                case 18 { v := 0xbbe1e08c35 }
                case 19 { v := 0xc5c55827cc }
                case 20 { v := 0xcfa8cfc363 }
                case 21 { v := 0xd98c475efa }
                case 22 { v := 0xe36fbefa91 }
                case 23 { v := 0xed53369628 }
                case 24 { v := 0xf736ae31bf }
                case 25 { v := 0x811a25cd56 }
                case 26 { v := 0x8afd9d68ed }
                case 27 { v := 0x94e1150484 }
                case 28 { v := 0x9ec48ca01b }
                case 29 { v := 0xa8a8043bb2 }
                case 30 { v := 0xb28b7bd749 }
                case 31 { v := 0xbc6ef372e0 }
                case 32 { v := 0xc6526b0e77 }
                case 33 { v := 0xd035e2aa0e }
                case 34 { v := 0xda195a45a5 }
                case 35 { v := 0xe3fcd1e13c }
                case 36 { v := 0xede0497cd3 }
                case 37 { v := 0xf7c3c1186a }
                case 38 { v := 0x81a738b401 }
                case 39 { v := 0x8b8ab04f98 }
                default { v := 0 }
            }
            for { let i := 0 } lt(i, n) { i := add(i, 1) } { s := add(s, pick(mod(i, 41))) }
        }
    }
}

contract OnceHelper {
    // CHECK-LABEL: OnceHelper ===
    // CHECK-NOT: switch {{.*}}, [0 =>
    function one(uint256 i) external pure returns (uint256 s) {
        assembly {
            function pick(i) -> v {
                switch i
                case 0 { v := 0x89e3779b97 }
                case 1 { v := 0x93c6ef372e }
                case 2 { v := 0x9daa66d2c5 }
                case 3 { v := 0xa78dde6e5c }
                case 4 { v := 0xb1715609f3 }
                case 5 { v := 0xbb54cda58a }
                case 6 { v := 0xc538454121 }
                case 7 { v := 0xcf1bbcdcb8 }
                case 8 { v := 0xd8ff34784f }
                case 9 { v := 0xe2e2ac13e6 }
                case 10 { v := 0xecc623af7d }
                case 11 { v := 0xf6a99b4b14 }
                case 12 { v := 0x808d12e6ab }
                case 13 { v := 0x8a708a8242 }
                case 14 { v := 0x9454021dd9 }
                case 15 { v := 0x9e3779b970 }
                case 16 { v := 0xa81af15507 }
                case 17 { v := 0xb1fe68f09e }
                case 18 { v := 0xbbe1e08c35 }
                case 19 { v := 0xc5c55827cc }
                case 20 { v := 0xcfa8cfc363 }
                case 21 { v := 0xd98c475efa }
                case 22 { v := 0xe36fbefa91 }
                case 23 { v := 0xed53369628 }
                case 24 { v := 0xf736ae31bf }
                case 25 { v := 0x811a25cd56 }
                case 26 { v := 0x8afd9d68ed }
                case 27 { v := 0x94e1150484 }
                case 28 { v := 0x9ec48ca01b }
                case 29 { v := 0xa8a8043bb2 }
                case 30 { v := 0xb28b7bd749 }
                case 31 { v := 0xbc6ef372e0 }
                case 32 { v := 0xc6526b0e77 }
                case 33 { v := 0xd035e2aa0e }
                case 34 { v := 0xda195a45a5 }
                case 35 { v := 0xe3fcd1e13c }
                case 36 { v := 0xede0497cd3 }
                case 37 { v := 0xf7c3c1186a }
                case 38 { v := 0x81a738b401 }
                case 39 { v := 0x8b8ab04f98 }
                default { v := 0 }
            }
            s := pick(i)
        }
    }
}
