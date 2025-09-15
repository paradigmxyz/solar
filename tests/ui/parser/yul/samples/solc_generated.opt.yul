/*
// solc +0.8.26 --ir-optimized ./c.sol
contract C {
    function f(uint256 x) public returns (uint256) {
        uint256 y = 1;
        y += 69;
        unchecked {
            y *= x;
        }
        y /= 64;
        return y;
    }
}
*/

/// @use-src 0:"c.sol"
object "C_28" {
    code {
        {
            /// @src 0:0:198  "contract C {..."
            mstore(64, memoryguard(0x80))
            if callvalue()
            {
                revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb()
            }
            let _1 := allocate_unbounded()
            codecopy(_1, dataoffset("C_28_deployed"), datasize("C_28_deployed"))
            return(_1, datasize("C_28_deployed"))
        }
        function allocate_unbounded() -> memPtr
        { memPtr := mload(64) }
        function revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb()
        { revert(0, 0) }
    }
    /// @use-src 0:"c.sol"
    object "C_28_deployed" {
        code {
            {
                /// @src 0:0:198  "contract C {..."
                mstore(64, memoryguard(0x80))
                if iszero(lt(calldatasize(), 4))
                {
                    let selector := shift_right_unsigned(calldataload(0))
                    switch selector
                    case 0xb3de648b { external_fun_f() }
                    default { }
                }
                revert_error_42b3090547df1d2001c96683413b8cf91c1b902ef5e3cb8d9f6f304cf7446f74()
            }
            function shift_right_unsigned(value) -> newValue
            { newValue := shr(224, value) }
            function allocate_unbounded() -> memPtr
            { memPtr := mload(64) }
            function revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb()
            { revert(0, 0) }
            function revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b()
            { revert(0, 0) }
            function cleanup_uint256(value) -> cleaned
            { cleaned := value }
            function validator_revert_uint256(value)
            {
                if iszero(eq(value, cleanup_uint256(value))) { revert(0, 0) }
            }
            function abi_decode_uint256(offset, end) -> value
            {
                value := calldataload(offset)
                validator_revert_uint256(value)
            }
            function abi_decode_tuple_uint256(headStart, dataEnd) -> value0
            {
                if slt(sub(dataEnd, headStart), 32)
                {
                    revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b()
                }
                let offset := 0
                value0 := abi_decode_uint256(add(headStart, offset), dataEnd)
            }
            function abi_encode_uint256_to_uint256(value, pos)
            {
                mstore(pos, cleanup_uint256(value))
            }
            function abi_encode_uint256(headStart, value0) -> tail
            {
                tail := add(headStart, 32)
                abi_encode_uint256_to_uint256(value0, add(headStart, 0))
            }
            function external_fun_f()
            {
                if callvalue()
                {
                    revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb()
                }
                let param := abi_decode_tuple_uint256(4, calldatasize())
                let ret := fun_f(param)
                let memPos := allocate_unbounded()
                let memEnd := abi_encode_uint256(memPos, ret)
                return(memPos, sub(memEnd, memPos))
            }
            function revert_error_42b3090547df1d2001c96683413b8cf91c1b902ef5e3cb8d9f6f304cf7446f74()
            { revert(0, 0) }
            function zero_value_for_split_uint256() -> ret
            { ret := 0 }
            function cleanup_rational_by(value) -> cleaned
            { cleaned := value }
            function identity(value) -> ret
            { ret := value }
            function convert_rational_1_by_1_to_uint256(value) -> converted
            {
                converted := cleanup_uint256(identity(cleanup_rational_by(value)))
            }
            function cleanup_t_rational_by(value) -> cleaned
            { cleaned := value }
            function convert_t_rational_by_to_t_uint256(value) -> converted
            {
                converted := cleanup_uint256(identity(cleanup_t_rational_by(value)))
            }
            function panic_error_0x11()
            {
                mstore(0, shl(224, 0x4e487b71))
                mstore(4, 0x11)
                revert(0, 0x24)
            }
            function checked_add_uint256(x, y) -> sum
            {
                x := cleanup_uint256(x)
                y := cleanup_uint256(y)
                sum := add(x, y)
                if gt(x, sum) { panic_error_0x11() }
            }
            function wrapping_mul_uint256(x, y) -> product
            {
                product := cleanup_uint256(mul(x, y))
            }
            function cleanup_rational_by_1(value) -> cleaned
            { cleaned := value }
            function convert_rational_by_to_uint256(value) -> converted
            {
                converted := cleanup_uint256(identity(cleanup_rational_by_1(value)))
            }
            function panic_error_0x12()
            {
                mstore(0, shl(224, 0x4e487b71))
                mstore(4, 0x12)
                revert(0, 0x24)
            }
            function checked_div_uint256(x, y) -> r
            {
                x := cleanup_uint256(x)
                y := cleanup_uint256(y)
                if iszero(y) { panic_error_0x12() }
                r := div(x, y)
            }
            /// @ast-id 27 @src 0:17:196  "function f(uint256 x) public returns (uint256) {..."
            function fun_f(var_x) -> var
            {
                /// @src 0:55:62  "uint256"
                let zero_uint256 := zero_value_for_split_uint256()
                var := zero_uint256
                /// @src 0:86:87  "1"
                let expr := 0x01
                /// @src 0:74:87  "uint256 y = 1"
                let var_y := convert_rational_1_by_1_to_uint256(expr)
                /// @src 0:102:104  "69"
                let expr_1 := 0x45
                /// @src 0:97:104  "y += 69"
                let _1 := convert_t_rational_by_to_t_uint256(expr_1)
                let _2 := var_y
                let expr_2 := checked_add_uint256(_2, _1)
                var_y := expr_2
                /// @src 0:143:144  "x"
                let _3 := var_x
                let expr_3 := _3
                /// @src 0:138:144  "y *= x"
                let _4 := var_y
                let expr_4 := wrapping_mul_uint256(_4, expr_3)
                var_y := expr_4
                /// @src 0:169:171  "64"
                let expr_5 := 0x40
                /// @src 0:164:171  "y /= 64"
                let _5 := convert_rational_by_to_uint256(expr_5)
                let _6 := var_y
                let expr_6 := checked_div_uint256(_6, _5)
                var_y := expr_6
                /// @src 0:188:189  "y"
                let _7 := var_y
                let expr_7 := _7
                /// @src 0:181:189  "return y"
                var := expr_7
                leave
            }
        }
        data ".metadata" hex"a2646970667358221220cb9774dc0239e8b062f7fc217650f80938f632767d6cc747e42e316bf835731064736f6c634300081a0033"
    }
}
