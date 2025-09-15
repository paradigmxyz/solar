/*
// solc +0.8.26 --ir ./c.sol
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
        /// @src 0:0:198  "contract C {..."
        mstore(64, memoryguard(128))
        if callvalue() { revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() }

        constructor_C_28()

        let _1 := allocate_unbounded()
        codecopy(_1, dataoffset("C_28_deployed"), datasize("C_28_deployed"))

        return(_1, datasize("C_28_deployed"))

        function allocate_unbounded() -> memPtr {
            memPtr := mload(64)
        }

        function revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() {
            revert(0, 0)
        }

        /// @src 0:0:198  "contract C {..."
        function constructor_C_28() {

            /// @src 0:0:198  "contract C {..."

        }
        /// @src 0:0:198  "contract C {..."

    }
    /// @use-src 0:"c.sol"
    object "C_28_deployed" {
        code {
            /// @src 0:0:198  "contract C {..."
            mstore(64, memoryguard(128))

            if iszero(lt(calldatasize(), 4))
            {
                let selector := shift_right_224_unsigned(calldataload(0))
                switch selector

                case 0xb3de648b
                {
                    // f(uint256)

                    external_fun_f_27()
                }

                default {}
            }

            revert_error_42b3090547df1d2001c96683413b8cf91c1b902ef5e3cb8d9f6f304cf7446f74()

            function shift_right_224_unsigned(value) -> newValue {
                newValue :=

                shr(224, value)

            }

            function allocate_unbounded() -> memPtr {
                memPtr := mload(64)
            }

            function revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() {
                revert(0, 0)
            }

            function revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b() {
                revert(0, 0)
            }

            function revert_error_c1322bf8034eace5e0b5c7295db60986aa89aae5e0ea0873e4689e076861a5db() {
                revert(0, 0)
            }

            function cleanup_t_uint256(value) -> cleaned {
                cleaned := value
            }

            function validator_revert_t_uint256(value) {
                if iszero(eq(value, cleanup_t_uint256(value))) { revert(0, 0) }
            }

            function abi_decode_t_uint256(offset, end) -> value {
                value := calldataload(offset)
                validator_revert_t_uint256(value)
            }

            function abi_decode_tuple_t_uint256(headStart, dataEnd) -> value0 {
                if slt(sub(dataEnd, headStart), 32) { revert_error_dbdddcbe895c83990c08b3492a0e83918d802a52331272ac6fdb6a7c4aea3b1b() }

                {

                    let offset := 0

                    value0 := abi_decode_t_uint256(add(headStart, offset), dataEnd)
                }

            }

            function abi_encode_t_uint256_to_t_uint256_fromStack(value, pos) {
                mstore(pos, cleanup_t_uint256(value))
            }

            function abi_encode_tuple_t_uint256__to_t_uint256__fromStack(headStart , value0) -> tail {
                tail := add(headStart, 32)

                abi_encode_t_uint256_to_t_uint256_fromStack(value0,  add(headStart, 0))

            }

            function external_fun_f_27() {

                if callvalue() { revert_error_ca66f745a3ce8ff40e2ccaf1ad45db7774001b90d25810abd9040049be7bf4bb() }
                let param_0 :=  abi_decode_tuple_t_uint256(4, calldatasize())
                let ret_0 :=  fun_f_27(param_0)
                let memPos := allocate_unbounded()
                let memEnd := abi_encode_tuple_t_uint256__to_t_uint256__fromStack(memPos , ret_0)
                return(memPos, sub(memEnd, memPos))

            }

            function revert_error_42b3090547df1d2001c96683413b8cf91c1b902ef5e3cb8d9f6f304cf7446f74() {
                revert(0, 0)
            }

            function zero_value_for_split_t_uint256() -> ret {
                ret := 0
            }

            function cleanup_t_rational_1_by_1(value) -> cleaned {
                cleaned := value
            }

            function identity(value) -> ret {
                ret := value
            }

            function convert_t_rational_1_by_1_to_t_uint256(value) -> converted {
                converted := cleanup_t_uint256(identity(cleanup_t_rational_1_by_1(value)))
            }

            function cleanup_t_rational_69_by_1(value) -> cleaned {
                cleaned := value
            }

            function convert_t_rational_69_by_1_to_t_uint256(value) -> converted {
                converted := cleanup_t_uint256(identity(cleanup_t_rational_69_by_1(value)))
            }

            function panic_error_0x11() {
                mstore(0, 35408467139433450592217433187231851964531694900788300625387963629091585785856)
                mstore(4, 0x11)
                revert(0, 0x24)
            }

            function checked_add_t_uint256(x, y) -> sum {
                x := cleanup_t_uint256(x)
                y := cleanup_t_uint256(y)
                sum := add(x, y)

                if gt(x, sum) { panic_error_0x11() }

            }

            function wrapping_mul_t_uint256(x, y) -> product {
                product := cleanup_t_uint256(mul(x, y))
            }

            function cleanup_t_rational_64_by_1(value) -> cleaned {
                cleaned := value
            }

            function convert_t_rational_64_by_1_to_t_uint256(value) -> converted {
                converted := cleanup_t_uint256(identity(cleanup_t_rational_64_by_1(value)))
            }

            function panic_error_0x12() {
                mstore(0, 35408467139433450592217433187231851964531694900788300625387963629091585785856)
                mstore(4, 0x12)
                revert(0, 0x24)
            }

            function checked_div_t_uint256(x, y) -> r {
                x := cleanup_t_uint256(x)
                y := cleanup_t_uint256(y)
                if iszero(y) { panic_error_0x12() }

                r := div(x, y)
            }

            /// @ast-id 27
            /// @src 0:17:196  "function f(uint256 x) public returns (uint256) {..."
            function fun_f_27(var_x_2) -> var__5 {
                /// @src 0:55:62  "uint256"
                let zero_t_uint256_1 := zero_value_for_split_t_uint256()
                var__5 := zero_t_uint256_1

                /// @src 0:86:87  "1"
                let expr_9 := 0x01
                /// @src 0:74:87  "uint256 y = 1"
                let var_y_8 := convert_t_rational_1_by_1_to_t_uint256(expr_9)
                /// @src 0:102:104  "69"
                let expr_12 := 0x45
                /// @src 0:97:104  "y += 69"
                let _2 := convert_t_rational_69_by_1_to_t_uint256(expr_12)
                let _3 := var_y_8
                let expr_13 := checked_add_t_uint256(_3, _2)

                var_y_8 := expr_13
                /// @src 0:143:144  "x"
                let _4 := var_x_2
                let expr_16 := _4
                /// @src 0:138:144  "y *= x"
                let _5 := var_y_8
                let expr_17 := wrapping_mul_t_uint256(_5, expr_16)

                var_y_8 := expr_17
                /// @src 0:169:171  "64"
                let expr_21 := 0x40
                /// @src 0:164:171  "y /= 64"
                let _6 := convert_t_rational_64_by_1_to_t_uint256(expr_21)
                let _7 := var_y_8
                let expr_22 := checked_div_t_uint256(_7, _6)

                var_y_8 := expr_22
                /// @src 0:188:189  "y"
                let _8 := var_y_8
                let expr_24 := _8
                /// @src 0:181:189  "return y"
                var__5 := expr_24
                leave

            }
            /// @src 0:0:198  "contract C {..."

        }

        data ".metadata" hex"a2646970667358221220cb9774dc0239e8b062f7fc217650f80938f632767d6cc747e42e316bf835731064736f6c634300081a0033"
    }

}
