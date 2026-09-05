//@ codegen-matrix: standard
//@ run-call: choose 0x1a64acf2 => 1
//@ run-call: choose 0x1a8f6dee => 2
//@ run-call: choose 0x1fd34797 => 3
//@ run-call: choose 0x5cc7bc10 => 4
//@ run-call: choose 0x671a7fae => 5
//@ run-call: choose 0x7535d246 => 6
//@ run-call: choose 0x88d51852 => 7
//@ run-call: choose 0x8da7fb18 => 8
//@ run-call: choose 0x9d2ffc1b => 9
//@ run-call: choose 0xb76398e4 => 10
//@ run-call: choose 0xfc0eed85 => 11
//@ run-call: choose 0xfed63a93 => 12
//@ run-call: choose 0 => 99
//@ run-call: choose 0x1a64acff => 99
//@ run-call: choose 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 99

// Sparse keys exercise every bucket and colliding non-case values.
contract ModuloBuckets {
    function choose(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 0x1a64acf2 { result := 1 }
            case 0x1a8f6dee { result := 2 }
            case 0x1fd34797 { result := 3 }
            case 0x5cc7bc10 { result := 4 }
            case 0x671a7fae { result := 5 }
            case 0x7535d246 { result := 6 }
            case 0x88d51852 { result := 7 }
            case 0x8da7fb18 { result := 8 }
            case 0x9d2ffc1b { result := 9 }
            case 0xb76398e4 { result := 10 }
            case 0xfc0eed85 { result := 11 }
            case 0xfed63a93 { result := 12 }
            default { result := 99 }
        }
    }
}
