// Test Yul variable scoping

contract YulScoping {
    uint256 public stateVar;
    mapping(uint256 => uint256) public map;

    // Test variable shadowing (Yul allows it)
    function testShadowing() public pure returns (uint256 result) {
        assembly {
            let x := 10
            {
                let x := 20  // shadows outer x
                result := x  // should be 20
            }
            result := add(result, x)  // outer x is still 10
        }
    }

    // Test nested block scoping
    function testNestedScopes() public pure returns (uint256 result) {
        assembly {
            let a := 1
            {
                let b := 2
                {
                    let c := 3
                    result := add(add(a, b), c)
                }
                // c is out of scope here
            }
            // b is out of scope here
        }
    }

    // Test for loop scoping - init block creates scope for entire loop
    function testForLoopScope() public pure returns (uint256 result) {
        assembly {
            result := 0
            for { let i := 0 } lt(i, 5) { i := add(i, 1) } {
                result := add(result, i)
            }
            // i is out of scope here
        }
    }

    // Test function scoping
    function testFunctionScope() public pure returns (uint256 result) {
        assembly {
            function double(x) -> y {
                y := mul(x, 2)
            }
            result := double(21)
        }
    }

    // Test function parameters and returns create their own scope
    function testFunctionParams() public pure returns (uint256 result) {
        assembly {
            function swap(a, b) -> x, y {
                x := b
                y := a
            }
            let p, q := swap(1, 2)
            result := add(p, q)
        }
    }

    // Test Yul functions are visible in entire block (hoisted)
    function testFunctionHoisting() public pure returns (uint256 result) {
        assembly {
            result := helper()  // can call before definition
            
            function helper() -> x {
                x := 42
            }
        }
    }

    // Test accessing Solidity variables
    function testSolidityVarAccess(uint256 param) public pure returns (uint256 result) {
        uint256 localVar := 10;
        assembly {
            result := add(param, localVar)
        }
    }

    // Test .slot access for storage variables  
    function testStorageSlot() public view returns (uint256 result) {
        assembly {
            result := sload(stateVar.slot)
        }
    }

    // Test .offset access for storage variables (in packed storage)
    function testStorageOffset() public pure returns (uint256) {
        assembly {
            let s := stateVar.slot
            let o := stateVar.offset
            mstore(0, add(s, o))
            return(0, 32)
        }
    }

    // Test .slot on mapping
    function testMappingSlot() public pure returns (bytes32 result) {
        uint256 key := 123;
        assembly {
            mstore(0, key)
            mstore(32, map.slot)
            result := keccak256(0, 64)
        }
    }
}
