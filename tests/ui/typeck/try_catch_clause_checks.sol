// A `try` statement only makes sense on a call that can revert on its own and hand back return
// data, and each `catch` clause decodes one fixed shape of that data, so a second clause of the
// same kind is dead and a differently typed parameter list decodes nothing.

interface I {
    function f() external returns (uint256);
}

struct S {
    uint256 a;
}

contract C {
    function internal_() internal returns (uint256) {
        return 1;
    }

    // Only an external call, a delegate call, and a contract creation call have a revert of
    // their own to catch. An internal call, a builtin, and a type conversion do not, and the
    // clauses of such a statement are not checked at all. A call of another kind is 2536.
    function target(address a, bytes memory d) external {
        try internal_() returns (uint256 v) {
            //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            v;
        } catch {}
        try abi.decode(d, (uint256)) returns (uint256 v) {
            //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            v;
        } catch {}
        try gasleft() returns (uint256 v) {
            //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            v;
        } catch {}
        try new bytes(4) returns (bytes memory b) {
            //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            b;
        } catch {}
    }

    // A target that is no call at all is 5347, and so are the two constructs that are calls in
    // the grammar only: a type conversion and a struct construction.
    function targetNotACall(address a, uint256 x) external {
        try I(a) returns (I i) {
            //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            i;
        } catch {}
        try uint256(x) returns (uint256 v) {
            //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            v;
        } catch {}
        try S(1) returns (S memory s) {
            //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            s;
        } catch {}
        try x++ returns (uint256 v) {
            //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            v;
        } catch {}
    }

    // A pruned function still has to pass semantic analysis: nothing reaches this call, so
    // lowering never sees it and only the checker can reject it.
    function pruned() external {
        if (false) {
            try internal_() {
                //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            } catch {}
        }
    }

    // Only one clause of each kind can ever run, so a second one is dead. A bare `catch` and
    // `catch (bytes memory)` are the same, low-level kind.
    function duplicates(address a) external {
        try I(a).f() returns (uint256 v) {
            v;
        } catch Error(string memory) {} catch Error(string memory) {}
        //~^ ERROR: this `try` statement already has an `Error` catch clause
        try I(a).f() returns (uint256 v) {
            v;
        } catch Panic(uint256) {} catch Panic(uint256) {}
        //~^ ERROR: this `try` statement already has a `Panic` catch clause
        try I(a).f() returns (uint256 v) {
            v;
        } catch (bytes memory) {} catch {}
        //~^ ERROR: this `try` statement already has a low-level catch clause
        try I(a).f() returns (uint256 v) {
            v;
        } catch {} catch (bytes memory) {}
        //~^ ERROR: this `try` statement already has a low-level catch clause
        try I(a).f() returns (uint256 v) {
            v;
        } catch {} catch {}
        //~^ ERROR: this `try` statement already has a low-level catch clause
    }

    // A clause takes exactly the one argument its error carries, or nothing for a bare `catch`.
    function parameters(address a) external {
        try I(a).f() returns (uint256 v) {
            v;
        } catch Error(uint256) {}
        //~^ ERROR: invalid `Error` catch clause parameters
        try I(a).f() returns (uint256 v) {
            v;
        } catch Panic(uint8) {}
        //~^ ERROR: invalid `Panic` catch clause parameters
        try I(a).f() returns (uint256 v) {
            v;
        } catch (uint256) {}
        //~^ ERROR: invalid low-level catch clause parameters
        try I(a).f() returns (uint256 v) {
            v;
        } catch (bytes memory, uint256) {}
        //~^ ERROR: invalid low-level catch clause parameters
    }

    // A rejected data location is reported once, on the declaration; solc rewrites it and does
    // not also complain about the clause's parameter type.
    function parameterLocation(address a) external {
        try I(a).f() returns (uint256 v) {
            v;
        } catch Error(string calldata) {}
        //~^ ERROR: invalid data location `calldata`
        try I(a).f() returns (uint256 v) {
            v;
        } catch (bytes storage) {}
        //~^ ERROR: invalid data location `storage`
    }

    // `Error` and `Panic` are the only clause names there are.
    function name(address a) external {
        try I(a).f() returns (uint256 v) {
            v;
        } catch Custom(uint256) {}
        //~^ ERROR: invalid catch clause name
    }

    // Valid companions: every accepted target, every clause kind once, and `uint` as the alias
    // of the `Panic` argument's type.
    function valid(address a) external {
        try I(a).f() returns (uint256 v) {
            v;
        } catch Error(string memory reason) {
            reason;
        } catch Panic(uint code) {
            code;
        } catch (bytes memory data) {
            data;
        }
        try new C() returns (C c) {
            c;
        } catch {}
        try this.valid(a) {} catch {}
    }
}
