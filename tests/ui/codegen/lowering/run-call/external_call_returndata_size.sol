//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: directOne => 32, 42
//@[none, gas, size] run-call: pointerOne => 32, 42
//@[none, gas, size] run-call: directTwo => 32, 42, 43
//@[none, gas, size] run-call: pointerTwo => 32, 42, 43
//@[none, gas, size] run-call-fail: directShort()
//@[none, gas, size] run-call-fail: pointerShort()
//@[none, gas, size] run-call-fail: directInvalidBool()
//@[none, gas, size] run-call-fail: pointerInvalidBool()

interface OneReturn {
    function f() external pure returns (uint256);
}

interface TwoReturns {
    function f() external pure returns (uint256, uint256);
}

interface BoolReturn {
    function f() external pure returns (bool);
}

contract LongReturn {
    function f() external pure returns (uint256[20] memory values) {
        values[0] = 42;
        values[1] = 43;
    }
}

contract ShortReturn {
    function f() external pure returns (uint256) {
        return 42;
    }
}

contract ExternalCallReturndataSize {
    function directOne() external returns (uint256 allocated, uint256 value) {
        address target = address(new LongReturn());
        uint256 before_;
        assembly {
            before_ := mload(0x40)
        }
        value = OneReturn(target).f();
        assembly {
            allocated := sub(mload(0x40), before_)
        }
    }

    function pointerOne() external returns (uint256 allocated, uint256 value) {
        address target = address(new LongReturn());
        function() external pure returns (uint256) call_ = OneReturn(target).f;
        uint256 before_;
        assembly {
            before_ := mload(0x40)
        }
        value = call_();
        assembly {
            allocated := sub(mload(0x40), before_)
        }
    }

    function directTwo()
        external
        returns (uint256 allocated, uint256 first, uint256 second)
    {
        address target = address(new LongReturn());
        uint256 before_;
        assembly {
            before_ := mload(0x40)
        }
        (first, second) = TwoReturns(target).f();
        assembly {
            allocated := sub(mload(0x40), before_)
        }
    }

    function pointerTwo()
        external
        returns (uint256 allocated, uint256 first, uint256 second)
    {
        address target = address(new LongReturn());
        function() external pure returns (uint256, uint256) call_ = TwoReturns(target).f;
        uint256 before_;
        assembly {
            before_ := mload(0x40)
        }
        (first, second) = call_();
        assembly {
            allocated := sub(mload(0x40), before_)
        }
    }

    function directShort() external returns (uint256, uint256) {
        return TwoReturns(address(new ShortReturn())).f();
    }

    function pointerShort() external returns (uint256, uint256) {
        function() external pure returns (uint256, uint256) call_ =
            TwoReturns(address(new ShortReturn())).f;
        return call_();
    }

    function directInvalidBool() external returns (bool) {
        return BoolReturn(address(new ShortReturn())).f();
    }

    function pointerInvalidBool() external returns (bool) {
        function() external pure returns (bool) call_ =
            BoolReturn(address(new ShortReturn())).f;
        return call_();
    }
}
