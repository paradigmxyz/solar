//@ run-call: useCaller 0x1111111111111111111111111111111111111111 => 0x2222222222222222222222222222222222222222, 0x1111111111111111111111111111111111111111
//@ run-call: useBalance 4; value=3 => 7
//@ run-call: useCall 5 => 47
//@ run-call: useYulFunction 6 => 7

// Yul calls resolve only to Yul functions and builtins, never to Solidity declarations.
contract YulBuiltinsShadowedBySolidity {
    function useCaller(address caller) external view returns (address result, address param) {
        assembly {
            result := caller()
        }
        param = caller;
    }

    function useBalance(uint256 balance) external payable returns (uint256 result) {
        assembly {
            result := balance(address())
        }
        result += balance;
    }

    function answer() external pure returns (uint256) {
        return 42;
    }

    function useCall(uint256 call) external returns (uint256 result) {
        bytes4 selector = this.answer.selector;
        assembly {
            mstore(0, selector)
            if iszero(call(gas(), address(), 0, 0, 4, 0, 32)) {
                revert(0, 0)
            }
            result := mload(0)
        }
        result += call;
    }

    function useYulFunction(uint256 shadowed) external pure returns (uint256 result) {
        assembly {
            function shadowed() -> r {
                r := 1
            }
            result := shadowed()
        }
        result += shadowed;
    }
}
