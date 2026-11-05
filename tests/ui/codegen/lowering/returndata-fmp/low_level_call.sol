//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: callEmpty() => 0
//@ run-call: staticcallEmpty() => 0
//@ run-call: delegatecallEmpty() => 0
//@ run-call: callWord() => 64
//@ run-call: staticcallWord() => 64
//@ run-call: delegatecallWord() => 64
//@ run-call: callRevertData() => 160
//@ run-call: stashedEmpty() => 0
//@ run-call: pickedEmpty() => 0
//@ run-call: forwardWord() => 1
//@ run-call: forwardEmpty()
//@ run-call-fail: forwardRevert() => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000568656c6c6f000000000000000000000000000000000000000000000000000000

// Empty returndata is the zero slot, so the free memory pointer does not move. Nonempty returndata
// is a length word and the padded data.
contract C {
    function noop() external {}

    function noopView() external view {}

    function word() external pure returns (uint256) {
        return 1;
    }

    function revertData() external pure {
        revert("hello");
    }

    function fmp() private pure returns (uint256 ptr) {
        assembly {
            ptr := mload(0x40)
        }
    }

    // CHECK-LABEL: fn @callEmpty()
    // CHECK: icall returndata_bytes<>
    function callEmpty() external returns (uint256) {
        bytes memory input = abi.encodeWithSelector(this.noop.selector);
        uint256 before = fmp();
        (bool ok, bytes memory data) = address(this).call(input);
        require(ok && data.length == 0);
        return fmp() - before;
    }

    function staticcallEmpty() external view returns (uint256) {
        bytes memory input = abi.encodeWithSelector(this.noopView.selector);
        uint256 before = fmp();
        (bool ok, bytes memory data) = address(this).staticcall(input);
        require(ok && data.length == 0);
        return fmp() - before;
    }

    function delegatecallEmpty() external returns (uint256) {
        bytes memory input = abi.encodeWithSelector(this.noop.selector);
        uint256 before = fmp();
        (bool ok, bytes memory data) = address(this).delegatecall(input);
        require(ok && data.length == 0);
        return fmp() - before;
    }

    function callWord() external returns (uint256) {
        bytes memory input = abi.encodeWithSelector(this.word.selector);
        uint256 before = fmp();
        (bool ok, bytes memory data) = address(this).call(input);
        require(ok && data.length == 32);
        return fmp() - before;
    }

    function staticcallWord() external view returns (uint256) {
        bytes memory input = abi.encodeWithSelector(this.word.selector);
        uint256 before = fmp();
        (bool ok, bytes memory data) = address(this).staticcall(input);
        require(ok && data.length == 32);
        return fmp() - before;
    }

    function delegatecallWord() external returns (uint256) {
        bytes memory input = abi.encodeWithSelector(this.word.selector);
        uint256 before = fmp();
        (bool ok, bytes memory data) = address(this).delegatecall(input);
        require(ok && data.length == 32);
        return fmp() - before;
    }

    function callRevertData() external returns (uint256) {
        bytes memory input = abi.encodeWithSelector(this.revertData.selector);
        uint256 before = fmp();
        (bool ok, bytes memory data) = address(this).call(input);
        require(!ok && data.length == 100);
        return fmp() - before;
    }

    // The returndata is only forwarded, so its empty object needs no zero slot.
    function forwardWord() external returns (uint256) {
        bytes memory input = abi.encodeWithSelector(this.word.selector);
        (bool ok, bytes memory data) = address(this).call(input);
        require(ok);
        assembly ("memory-safe") {
            return(add(data, 32), mload(data))
        }
    }

    function forwardEmpty() external {
        bytes memory input = abi.encodeWithSelector(this.noop.selector);
        (bool ok, bytes memory data) = address(this).call(input);
        require(ok);
        assembly ("memory-safe") {
            return(add(data, 32), mload(data))
        }
    }

    function forwardRevert() external {
        bytes memory input = abi.encodeWithSelector(this.revertData.selector);
        (bool ok, bytes memory data) = address(this).call(input);
        require(!ok);
        assembly ("memory-safe") {
            revert(add(data, 32), mload(data))
        }
    }

    function stash() private {
        bytes memory input = abi.encodeWithSelector(this.noop.selector);
        (bool ok, bytes memory data) = address(this).call(input);
        require(ok);
        assembly {
            sstore(0, data)
        }
    }

    // A pointer saved to storage outlives the call, so the empty object cannot sit at the free
    // memory pointer, where the next allocation overwrites its length.
    function stashedEmpty() external returns (uint256 length) {
        stash();
        bytes memory other = abi.encode(uint256(1), uint256(2));
        require(other.length == 64);
        bytes memory data;
        assembly {
            data := sload(0)
        }
        return data.length;
    }

    function pick(bool useData) private returns (bytes memory picked) {
        bytes memory input = abi.encodeWithSelector(this.noop.selector);
        (bool ok, bytes memory data) = address(this).call(input);
        require(ok);
        picked = useData ? data : bytes("x");
    }

    // The object leaves `pick` through a phi, so it must not sit at the free memory pointer.
    function pickedEmpty() external returns (uint256) {
        bytes memory data = pick(true);
        bytes memory other = abi.encode(uint256(1), uint256(2));
        require(other.length == 64);
        return data.length;
    }
}
