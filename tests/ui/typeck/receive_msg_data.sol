// ported-from: test/libsolidity/syntaxTests/receiveEther/msg_data_in_receive.sol

contract C {
    receive() external payable { msg.data; } //~ ERROR: `msg.data` cannot be used inside of a `receive` function
}

contract Members {
    receive() external payable {
        uint256 length = msg.data.length; //~ ERROR: `msg.data` cannot be used inside of a `receive` function
        bytes calldata data = msg.data[0:0]; //~ ERROR: `msg.data` cannot be used inside of a `receive` function
        length;
        data;
    }
}

// Only the body of `receive` itself is checked.
contract Allowed {
    modifier m() {
        msg.data; //~ WARN: statement has no effect
        _;
    }

    receive() external payable m {
        msg.sig; //~ WARN: statement has no effect
        f();
    }

    fallback() external {
        msg.data; //~ WARN: statement has no effect
    }

    function f() internal pure {
        msg.data; //~ WARN: statement has no effect
    }
}
