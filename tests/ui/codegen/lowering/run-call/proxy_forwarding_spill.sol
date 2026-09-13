//@ codegen-matrix: standard
//@ run-call: Harness::run => 1
//@ run-call: Harness::returnedData => 1
//@ run-call: Harness::historicalReturnData => 1
//@ run-call: Harness::guardedReturnData => 1
//@ run-call: TypedCopyCall::run => true
//@ run-call: Harness::helperClobber => 1
//@ run-call: Harness::selectorLiveOut => 1
//@ run-call: Harness::largeSwitch => 1
//@ run-call: Harness::internalDelegate => 1
//@ run-call: Harness::manyLiveValues => 1
//@ run-call: Harness::postCopyDeepSpills => 1

// A forwarding proxy sets the free-memory pointer low and
// `calldatacopy(0, 0, calldatasize())` before `delegatecall`ing the copied
// calldata. The compiler spilled the implementation address to a low slot the
// copy overwrites, then reloaded it after the copy — delegatecalling a garbage
// (zero) address. With calldata under one word the slots are missed and the
// proxy works, so the bug only shows for a >128-byte forwarded call. The
// backend must keep the value stack-resident across the clobber and never
// stage the call's own operands into its forwarded input range.

contract Impl {
    uint256 public v;

    function payload() external pure returns (bytes memory data) {
        data = new bytes(1024);
        for (uint256 i; i < data.length; ++i) {
            data[i] = bytes1(uint8(i));
        }
    }

    function setBig(uint256 a, uint256 b, uint256 c, bytes calldata d) external {
        v = a + b + c + d.length;
    }
}

contract Proxy {
    bytes32 internal immutable _defaultImplementation;
    bytes32 internal constant _SLOT = 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc;

    constructor(address impl) {
        _defaultImplementation = bytes32(uint256(uint160(impl)));
    }

    fallback() external payable {
        bytes32 implementation;
        assembly {
            mstore(0x40, returndatasize())
            implementation := sload(_SLOT)
        }
        if (implementation == bytes32(0)) {
            implementation = _defaultImplementation;
        }
        assembly {
            calldatacopy(returndatasize(), returndatasize(), calldatasize())
            if iszero(
                delegatecall(gas(), implementation, returndatasize(), calldatasize(), codesize(), returndatasize())
            ) {
                returndatacopy(0x00, 0x00, returndatasize())
                revert(0x00, returndatasize())
            }
            returndatacopy(0x00, 0x00, returndatasize())
            return(0x00, returndatasize())
        }
    }
}

contract GuardedReturnData {
    uint256 value = 37;

    function run(address target) external returns (uint256) {
        assembly { if iszero(call(gas(), target, 0, 0, 0, 0, 0)) { revert(0, 0) } }
        uint256 saved = value;
        require(check());
        return saved;
    }

    function check() private pure returns (bool success) {
        assembly {
            switch returndatasize()
            case 32 {
                returndatacopy(0, 0, returndatasize())
                success := iszero(iszero(mload(0)))
            }
        }
    }
}

contract Harness {
    function guardedReturnData() external returns (uint256) {
        GuardedReturnData receiver = new GuardedReturnData();
        require(receiver.run(address(new CalldataHash())) == 37);
        return 1;
    }

    function helperClobber() external returns (uint256) {
        CallerClobber proxy = new CallerClobber();
        bytes memory data = new bytes(1024);
        for (uint256 i; i < data.length; ++i) data[i] = bytes1(uint8(i));
        (bool ok, bytes memory result) = address(proxy).call(data);
        require(ok && abi.decode(result, (uint256)) == 0);
        require(proxy.heapCopy() == CallerClobber.heapCopy.selector);
        return 1;
    }

    function postCopyDeepSpills() external returns (uint256) {
        CalldataHash target = new CalldataHash();
        DeepSpillProxy proxy = new DeepSpillProxy(address(target));
        bytes memory data = new bytes(1024);
        for (uint256 i; i < data.length; ++i) data[i] = bytes1(uint8(i));
        (bool directOk, bytes memory direct) = address(target).call(data);
        (bool proxyOk, bytes memory forwarded) = address(proxy).call(data);
        require(directOk && proxyOk && keccak256(direct) == keccak256(forwarded));
        DeepStorageSpillProxy storageProxy = new DeepStorageSpillProxy(address(target));
        (bool storageOk, bytes memory storageForwarded) = address(storageProxy).call(data);
        require(storageOk && keccak256(direct) == keccak256(storageForwarded));
        return 1;
    }

    function manyLiveValues() external returns (uint256) {
        ManyLiveValues receiver = new ManyLiveValues();
        (bool ok,) = address(receiver).call(abi.encode(uint256(1), 2, 3, 4, 5, 6, 7, 8, 9));
        require(ok && receiver.sum() == 45);
        return 1;
    }

    function internalDelegate() external returns (uint256) {
        Impl impl = new Impl();
        InternalDelegateProxy proxy = new InternalDelegateProxy(address(impl));
        Impl(address(proxy)).setBig(1, 2, 3, new bytes(4096));
        require(Impl(address(proxy)).v() == 4102);
        return 1;
    }

    function largeSwitch() external returns (uint256) {
        Impl impl = new Impl();
        LargeSwitchProxy proxy = new LargeSwitchProxy(address(impl));
        require(keccak256(Impl(address(proxy)).payload()) == keccak256(impl.payload()));
        return 1;
    }

    function selectorLiveOut() external returns (uint256) {
        Impl impl = new Impl();
        SelectorProxy proxy = new SelectorProxy(address(impl));
        require(keccak256(Impl(address(proxy)).payload()) == keccak256(impl.payload()));
        return 1;
    }

    function historicalReturnData() external returns (uint256) {
        Impl impl = new Impl();
        HistoricalReturnProxy proxy = new HistoricalReturnProxy(address(impl));
        require(keccak256(Impl(address(proxy)).payload()) == keccak256(impl.payload()));
        return 1;
    }

    function returnedData() external returns (uint256) {
        Impl impl = new Impl();
        ReturnProxy proxy = new ReturnProxy(address(impl));
        require(keccak256(Impl(address(proxy)).payload()) == keccak256(impl.payload()));
        return 1;
    }

    function run() external returns (uint256) {
        Impl impl = new Impl();
        Proxy p = new Proxy(address(impl));
        // 4 + 3*32 (a,b,c) + 32 (offset) + 32 (len) = 196 bytes > 128, so the
        // forwarded copy overwrites the compiler's low spill slots.
        Impl(address(p)).setBig(10, 20, 30, hex"deadbeefdeadbeefdeadbeefdeadbeef");
        require(Impl(address(p)).v() == 76, "forward");
        return 1;
    }
}

contract ReturnProxy {
    address immutable implementation;

    constructor(address impl) {
        implementation = impl;
    }

    fallback() external {
        address impl = implementation;
        assembly {
            calldatacopy(0, 0, calldatasize())
            let success := delegatecall(gas(), impl, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            switch success
            case 0 { revert(0, returndatasize()) }
            default { return(0, returndatasize()) }
        }
    }
}

contract HistoricalReturnProxy {
    address immutable implementation;

    constructor(address impl) {
        implementation = impl;
    }

    fallback() external {
        address impl = implementation;
        assembly {
            mstore(0, calldataload(0))
            let saved := sload(0)
            let oldSize := returndatasize()
            switch oldSize
            case 0 {
                let success := delegatecall(gas(), impl, 0, calldatasize(), 0, 0)
                returndatacopy(0, oldSize, returndatasize())
                if saved { sstore(1, saved) }
                switch success
                case 0 { revert(0, returndatasize()) }
                default { return(0, returndatasize()) }
            }
            default { revert(0, 0) }
        }
    }
}

contract LargeSwitchProxy {
    address immutable implementation;

    constructor(address impl) { implementation = impl; }

    fallback() external {
        address impl = implementation;
        assembly {
            calldatacopy(0, 0, calldatasize())
            pop(delegatecall(gas(), impl, 0, calldatasize(), 0, 0))
            let size := returndatasize()
            returndatacopy(0, 0, size)
            switch calldataload(0)
            case 0 { return(0, add(size, 1)) }
            case 1 { return(0, add(size, 2)) }
            case 2 { return(0, add(size, 3)) }
            case 3 { return(0, add(size, 4)) }
            case 4 { return(0, add(size, 5)) }
            case 5 { return(0, add(size, 6)) }
            case 6 { return(0, add(size, 7)) }
            case 7 { return(0, add(size, 8)) }
            case 8 { return(0, add(size, 9)) }
            case 9 { return(0, add(size, 10)) }
            case 10 { return(0, add(size, 11)) }
            case 11 { return(0, add(size, 12)) }
            case 12 { return(0, add(size, 13)) }
            case 13 { return(0, add(size, 14)) }
            case 14 { return(0, add(size, 15)) }
            case 15 { return(0, add(size, 16)) }
            default { return(0, size) }
        }
    }
}

contract SelectorProxy {
    address immutable implementation;

    constructor(address impl) { implementation = impl; }

    fallback() external {
        address impl = implementation;
        bytes4 selector = msg.sig;
        assembly {
            calldatacopy(0, 0, calldatasize())
            let success := delegatecall(gas(), impl, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            switch success
            case 0 { revert(0, returndatasize()) }
            default {
                if iszero(selector) { revert(0, 0) }
                return(0, returndatasize())
            }
        }
    }
}

contract InternalDelegateProxy {
    address immutable implementation;

    constructor(address impl) { implementation = impl; }

    fallback() external { _delegate(implementation); }

    function _delegate(address target) internal {
        assembly {
            calldatacopy(0, 0, calldatasize())
            if iszero(calldatasize()) { return(0, 0) }
            let success := delegatecall(gas(), target, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            switch success
            case 0 { revert(0, returndatasize()) }
            default { return(0, returndatasize()) }
        }
    }
}

contract ManyLiveValues {
    fallback() external {
        assembly {
            mstore(0x40, 0)
            let x0 := calldataload(0)
            let x1 := calldataload(32)
            let x2 := calldataload(64)
            let x3 := calldataload(96)
            let x4 := calldataload(128)
            let x5 := calldataload(160)
            let x6 := calldataload(192)
            let x7 := calldataload(224)
            let x8 := calldataload(256)
            calldatacopy(0, 0, calldatasize())
            if iszero(calldataload(0)) { revert(0, 0) }
            sstore(0, x0)
            sstore(1, x1)
            sstore(2, x2)
            sstore(3, x3)
            sstore(4, x4)
            sstore(5, x5)
            sstore(6, x6)
            sstore(7, x7)
            sstore(8, x8)
        }
    }

    function sum() external view returns (uint256 result) {
        assembly {
            result := add(add(add(add(add(add(add(add(sload(0), sload(1)), sload(2)), sload(3)), sload(4)), sload(5)), sload(6)), sload(7)), sload(8))
        }
    }
}

contract CalldataHash {
    fallback() external {
        assembly {
            calldatacopy(0, 0, calldatasize())
            mstore(0, keccak256(0, calldatasize()))
            return(0, 32)
        }
    }
}

contract DeepSpillProxy {
    address immutable implementation;

    constructor(address target) { implementation = target; }

    fallback() external {
        address target = implementation;
        assembly {
            calldatacopy(0, 0, calldatasize())
            let x0 := calldataload(0)
            let x1 := calldataload(32)
            let x2 := calldataload(64)
            let x3 := calldataload(96)
            let x4 := calldataload(128)
            let x5 := calldataload(160)
            let x6 := calldataload(192)
            let x7 := calldataload(224)
            let x8 := calldataload(256)
            let x9 := calldataload(288)
            let x10 := calldataload(320)
            let x11 := calldataload(352)
            let x12 := calldataload(384)
            let x13 := calldataload(416)
            let x14 := calldataload(448)
            let x15 := calldataload(480)
            let x16 := calldataload(512)
            let x17 := calldataload(544)
            sstore(0, x0)
            sstore(1, x1)
            sstore(2, x2)
            sstore(3, x3)
            sstore(4, x4)
            sstore(5, x5)
            sstore(6, x6)
            sstore(7, x7)
            sstore(8, x8)
            sstore(9, x9)
            sstore(10, x10)
            sstore(11, x11)
            sstore(12, x12)
            sstore(13, x13)
            sstore(14, x14)
            sstore(15, x15)
            sstore(16, x16)
            sstore(17, x17)
            let ok := delegatecall(gas(), target, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            if iszero(ok) { revert(0, returndatasize()) }
            return(0, returndatasize())
        }
    }
}

contract DeepStorageSpillProxy {
    // A separate external entry does not observe the fallback's memory.
    function memorySize() external pure returns (uint256 size) {
        assembly { size := msize() }
    }

    address immutable implementation;

    constructor(address target) { implementation = target; }

    fallback() external {
        address target = implementation;
        assembly {
            calldatacopy(0, 0, calldatasize())
            let x0 := sload(0)
            let x1 := sload(1)
            let x2 := sload(2)
            let x3 := sload(3)
            let x4 := sload(4)
            let x5 := sload(5)
            let x6 := sload(6)
            let x7 := sload(7)
            let x8 := sload(8)
            let x9 := sload(9)
            let x10 := sload(10)
            let x11 := sload(11)
            let x12 := sload(12)
            let x13 := sload(13)
            let x14 := sload(14)
            let x15 := sload(15)
            let x16 := sload(16)
            let x17 := sload(17)
            sstore(0, x0)
            sstore(1, x1)
            sstore(2, x2)
            sstore(3, x3)
            sstore(4, x4)
            sstore(5, x5)
            sstore(6, x6)
            sstore(7, x7)
            sstore(8, x8)
            sstore(9, x9)
            sstore(10, x10)
            sstore(11, x11)
            sstore(12, x12)
            sstore(13, x13)
            sstore(14, x14)
            sstore(15, x15)
            sstore(16, x16)
            sstore(17, x17)
            let ok := delegatecall(gas(), target, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            if iszero(ok) { revert(0, returndatasize()) }
            return(0, returndatasize())
        }
    }
}

contract CallerClobber {
    function heapCopy() external pure returns (bytes4) {
        bytes memory data = new bytes(32);
        uint256 dest;
        assembly { dest := add(data, 32) }
        copy(1, dest);
        return bytes4(data);
    }

    fallback() external {
        uint256 a;
        uint256 b;
        uint256 c;
        uint256 d;
        uint256 e;
        uint256 f;
        uint256 g;
        uint256 h;
        uint256 i;
        assembly {
            a := sload(0)
            b := sload(1)
            c := sload(2)
            d := sload(3)
            e := sload(4)
            f := sload(5)
            g := sload(6)
            h := sload(7)
            i := sload(8)
        }
        uint256 count;
        unchecked { count = msg.data.length + 1; }
        uint256 copied = copy(count, 0);
        assembly {
            mstore(0, add(sub(copied, count), add(add(add(add(add(add(add(add(a, b), c), d), e), f), g), h), i)))
            return(0, 32)
        }
    }

    function copy(uint256 count, uint256 dest) internal pure returns (uint256) {
        copyInto(dest);
        return count;
    }

    function copyInto(uint256 dest) private pure {
        assembly { calldatacopy(dest, 0, calldatasize()) }
    }
}

interface TypedCopyToken {}

contract TypedCopyCall {
    function other(TypedCopyToken token, address to, uint256 amount) external returns (bool) {
        return TypedCopyLib.forward(token, to, amount);
    }

    function run() external returns (bool) {
        require(TypedCopyLib.forward(TypedCopyToken(address(0xBADBEEF)), address(0xBEEF), 1e18));
        require(TypedCopyLib.forward(TypedCopyToken(address(0xBADBEEF)), address(0xFEED), 2e18));
        return true;
    }

}

library TypedCopyLib {
    function forward(TypedCopyToken target, address to, uint256 amount) internal returns (bool) {
        bool success;
        assembly {
            let p := mload(0x40)
            mstore(p, 0xa9059cbb00000000000000000000000000000000000000000000000000000000)
            mstore(add(p, 4), and(to, 0xffffffffffffffffffffffffffffffffffffffff))
            mstore(add(p, 36), amount)
            success := call(gas(), target, 0, p, 68, 0, 0)
        }
        return check(success);
    }

    function check(bool success) private pure returns (bool result) {
        assembly {
            let size := returndatasize()
            if iszero(success) {
                returndatacopy(0, 0, size)
                revert(0, size)
            }
            switch size
            case 0 { result := 1 }
            case 32 {
                returndatacopy(0, 0, size)
                result := iszero(iszero(mload(0)))
            }
        }
    }
}
