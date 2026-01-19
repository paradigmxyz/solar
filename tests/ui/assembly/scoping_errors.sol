// Test Yul scoping error cases

contract YulScopingErrors {
    uint256 public stateVar;
    uint256 localInContract;

    function testUndefinedVariable() public pure {
        assembly {
            let x := undefinedVar  //~ ERROR: undefined variable `undefinedVar`
        }
    }

    function testUndefinedFunction() public pure {
        assembly {
            let x := unknownFunc()  //~ ERROR: undefined function `unknownFunc`
        }
    }

    function testVarOutOfScope() public pure {
        assembly {
            {
                let inner := 42
            }
            let x := inner  //~ ERROR: undefined variable `inner`
        }
    }

    function testSlotOnYulVar() public pure {
        assembly {
            let x := 10
            let s := x.slot  //~ ERROR: Yul variable `x` cannot have `.slot` suffix
        }
    }

    function testOffsetOnYulVar() public pure {
        assembly {
            let y := 20
            let o := y.offset  //~ ERROR: Yul variable `y` cannot have `.offset` suffix
        }
    }

    function testSlotOnLocalVar() public pure {
        uint256 localVar := 10;
        assembly {
            let s := localVar.slot  //~ ERROR: `.slot` is only allowed on storage variables
        }
    }

    function testSlotOnParam(uint256 param) public pure {
        assembly {
            let s := param.slot  //~ ERROR: `.slot` is only allowed on storage variables
        }
    }

    function testInvalidSuffix() public pure {
        assembly {
            let s := stateVar.invalid  //~ ERROR: unknown suffix `.invalid`
        }
    }

    function testDuplicateVarDecl() public pure {
        assembly {
            let x := 1
            let x := 2  //~ ERROR: variable `x` already declared
        }
    }

    function testDuplicateFuncDecl() public pure {
        assembly {
            function foo() {}
            function foo() {}  //~ ERROR: function `foo` already declared
        }
    }

    function testAccessContract() public pure {
        assembly {
            let x := YulScopingErrors  //~ ERROR: cannot access contract
        }
    }
}
