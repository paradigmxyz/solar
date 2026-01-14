//@compile-flags: -Zstatic-analysis
// Static analyzer warnings test

contract TestUnused {
    uint stateVar = 1;

    // Test: Unused function parameter
    function unusedParam(uint x) public pure { //~ WARN: unused function parameter
    }

    // Test: Unused local variable
    function unusedLocal() public pure {
        uint y = 5; //~ WARN: unused local variable
    }

    // Test: Used variable - no warning
    function usedLocal() public pure returns (uint) {
        uint z = 5;
        return z;
    }
}

contract TestShadowing {
    uint stateVar = 42;

    // Test: Local shadows state variable and unused
    function shadowState() public pure {
        uint stateVar = 10; //~ WARN: shadows a state variable
    } //~^ WARN: unused local variable

    // Test: Parameter shadows state variable and unused
    function shadowParam(uint stateVar) public pure { //~ WARN: shadows a state variable
    } //~^ WARN: unused function parameter
}

contract TestDivision {
    // Test: Division by zero constant
    function divByZero() public pure returns (uint) {
        uint x = 10;
        return x / 0; //~ ERROR: division by zero
    }

    // Test: Modulo by zero constant
    function modByZero() public pure returns (uint) {
        uint x = 10;
        return x % 0; //~ ERROR: modulo zero
    }
}

contract TestSelfAssignment {
    // Test: Self-assignment
    function selfAssign() public pure {
        uint x = 5;
        x = x; //~ WARN: self-assignment
    }
}

contract TestBooleanComparison {
    // Test: Boolean constant comparison with true
    function boolTrue(bool a) public pure returns (bool) {
        return a == true; //~ WARN: comparison with boolean constant
    }

    // Test: Boolean constant comparison with false
    function boolFalse(bool a) public pure returns (bool) {
        return a == false; //~ WARN: comparison with boolean constant
    }
}

contract TestStatementNoEffect {
    // Test: Statement with no effect
    function noEffect() public pure {
        5; //~ WARN: statement has no effect
        1 + 2; //~ WARN: statement has no effect
    }
}

contract TestAssertRequireMessage {
    // Test: Assert without message
    function assertNoMsg(bool b) public pure {
        assert(b); //~ WARN: assertion without description
    }

    // Test: Require without message
    function requireNoMsg(bool b) public pure {
        require(b); //~ WARN: require without error message
    }

    // Test: Assert with message - no warning
    function assertWithMsg(bool b) public pure {
        assert(b, "condition failed");
    }

    // Test: Require with message - no warning
    function requireWithMsg(bool b) public pure {
        require(b, "condition failed");
    }
}

contract TestAddModMulMod {
    // Test: addmod with zero modulo
    function addmodZero() public pure returns (uint) {
        return addmod(1, 2, 0); //~ ERROR: arithmetic modulo zero
    }

    // Test: mulmod with zero modulo
    function mulmodZero() public pure returns (uint) {
        return mulmod(1, 2, 0); //~ ERROR: arithmetic modulo zero
    }

    // Test: addmod with non-zero - no warning
    function addmodOk() public pure returns (uint) {
        return addmod(1, 2, 3);
    }
}
