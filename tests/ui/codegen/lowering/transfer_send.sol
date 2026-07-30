//@ run-call: TransferSend::sendValue(); value=1 => false
//@ run-call-fail: TransferSend::transferValue(); value=1 => 0xdeadbeef

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

    function sendValue() external payable returns (bool) {
        return payable(address(gasReceiver)).send(1);
    }

    function transferValue() external payable {
        payable(address(revertReceiver)).transfer(1);
    }
}
