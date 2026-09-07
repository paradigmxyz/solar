//@ codegen-matrix: standard
//@ filecheck:
// CHECK: @module TransferSend
//@ run-call: TransferSend::sendValue; value=1 => false
//@ run-call-fail: TransferSend::transferValue; value=1 => 0xdeadbeef

//@ run-call: TransferSend::sendZero => true
//@ run-call: TransferSend::sendInsufficient => false
//@ run-call: TransferSend::balances; value=2 => 2, 1, 0
//@ run-call: TransferSend::sendRevertData => 0xdeadbeef
//@ run-call-fail: TransferSend::transferOutOfGas; value=1 => 0x
//@ run-call-fail: TransferSend::transferInsufficient => 0x

contract GasReceiver {
    receive() external payable {
        assembly {
            log0(0, 0)
            log0(0, 0)
            log0(0, 0)
            log0(0, 0)
            log0(0, 0)
            log0(0, 0)
            log0(0, 0)
        }
    }
}

contract RevertReceiver {
    receive() external payable {
        assembly {
            mstore(0, shl(224, 0xdeadbeef))
            revert(0, 4)
        }
    }
}

contract TransferSend {
    GasReceiver private gasReceiver;
    RevertReceiver private revertReceiver;

    constructor() {
        gasReceiver = new GasReceiver();
        revertReceiver = new RevertReceiver();
    }

    // CHECK-LABEL: fn @sendValue
    // CHECK: = send
    function sendValue() external payable returns (bool) {
        return payable(address(gasReceiver)).send(1);
    }

    // CHECK-LABEL: fn @transferValue
    // CHECK: transfer
    function transferValue() external payable {
        payable(address(revertReceiver)).transfer(1);
    }

    function sendZero() external returns (bool) {
        return payable(address(0xbeef)).send(0);
    }

    function sendInsufficient() external returns (bool) {
        return payable(address(0xbeef)).send(1);
    }

    function balances() external payable returns (uint256 before_, uint256 middle, uint256 after_) {
        before_ = address(this).balance;
        require(payable(address(0xbeef)).send(1));
        middle = address(this).balance;
        payable(address(0xbeef)).transfer(1);
        after_ = address(this).balance;
    }

    function sendRevertData() external returns (bytes4 data) {
        payable(address(revertReceiver)).send(0);
        assembly {
            returndatacopy(0, 0, 4)
            data := mload(0)
        }
    }

    function transferOutOfGas() external payable {
        payable(address(gasReceiver)).transfer(1);
    }

    function transferInsufficient() external {
        payable(address(0xbeef)).transfer(1);
    }
}
