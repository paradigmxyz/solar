// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/if_declaration_err.sol
// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/short_circuit_declaration_err.sol
// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/ternary_declaration_fine.sol
// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/while_declaration_fine.sol
// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/dowhile_declaration_fine.sol
// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/if_err.sol

contract StorageReferenceDefiniteAssignment {
    struct Status {
        uint256 remaining;
    }

    mapping(uint256 => Status) internal statuses;

    function unassigned() internal {
        Status storage status;
        status.remaining = 1; //~ ERROR: storage pointer variable can be accessed before assignment
    }

    function oneBranch(bool useFirst) internal {
        Status storage status;
        if (useFirst) {
            status = statuses[1];
        }
        status.remaining = 2; //~ ERROR: storage pointer variable can be accessed before assignment
    }

    function shortCircuit(bool assign) internal {
        Status storage status;
        assign && (status = statuses[1]).remaining != 0;
        status.remaining = 3; //~ ERROR: storage pointer variable can be accessed before assignment
    }

    function bothBranches(bool useFirst) internal {
        Status storage status;
        if (useFirst) {
            status = statuses[1];
        } else {
            status = statuses[2];
        }
        status.remaining = 4;
    }

    function ternary(bool useFirst) internal {
        Status storage status;
        useFirst ? status = statuses[1] : status = statuses[2];
        status.remaining = 5;
    }

    function whileCondition() internal {
        Status storage status;
        while ((status = statuses[1]).remaining != 0) {
            break;
        }
        status.remaining = 6;
    }

    function doWhileBody() internal {
        Status storage status;
        do {
            status = statuses[1];
        } while (false);
        status.remaining = 7;
    }

    function unassignedReturn(bool assign)
        internal
        returns (Status storage status) //~ ERROR: storage pointer variable can be returned before assignment
    {
        if (assign) {
            status = statuses[1];
        }
    }

    function assignedReturn(bool useFirst) internal returns (Status storage status) {
        if (useFirst) {
            status = statuses[1];
        } else {
            status = statuses[2];
        }
    }
}
