// ported-from: test/libyul/yulSyntaxTests/break_outside_of_for_loop.yul
// ported-from: test/libyul/yulSyntaxTests/continue_outside_of_for_loop.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_break.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_break_init.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_break_nested_body_in_init.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_break_nested_body_in_post.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_break_post.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_continue.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_continue_fail_init.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_continue_fail_post.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_continue_nested_body_in_init.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_continue_nested_body_in_post.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_continue_nested_init_in_body.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_nested_break.yul
// ported-from: test/libyul/yulSyntaxTests/for_statement_nested_continue.yul

contract C {
    function test() public {
        assembly {
            {
                let x
                if x { break } //~ ERROR: keyword `break` needs to be inside a for-loop body
            }
            {
                let x
                if x { continue } //~ ERROR: keyword `continue` needs to be inside a for-loop body
            }
            {
                let x
                for {let i := 0} x {i := add(i, 1)}
                {
                    break
                }
            }
            {
                let x
                for {let i := 0 break} x {i := add(i, 1)} {} //~ ERROR: keyword `break` in for-loop init block is not allowed
            }
            {
                for {let x for {} x {} { break }} 1 {}
                {}
            }
            {
                for {} 1 {let x for {} x {} { break }}
                {}
            }
            {
                let x
                for {let i := 0 } x {i := add(i, 1) break} {} //~ ERROR: keyword `break` in for-loop post block is not allowed
            }
            {
                let x
                for {let i := 0} x {i := add(i, 1)}
                {
                    continue
                }
            }
            {
                let x
                for {let i := 0 continue} x {i := add(i, 1)} //~ ERROR: keyword `continue` in for-loop init block is not allowed
                {
                }
            }
            {
                let x
                for {let i := 0} x {i := add(i, 1) continue} {} //~ ERROR: keyword `continue` in for-loop post block is not allowed
            }
            {
                for {let x for {} x {} { continue }} 1 {}
                {
                }
            }
            {
                for {} 1 {let x for {} x {} { continue }}
                {}
            }
            {
                for {} 1 {}
                {
                    let x
                    for { continue } x {} {} //~ ERROR: keyword `continue` in for-loop init block is not allowed
                }
            }
            {
                let x
                for {let i := 0} x {}
                {
                    function f() { break } //~ ERROR: keyword `break` needs to be inside a for-loop body
                }
            }
            {
                for {let i := 0} iszero(eq(i, 10)) {}
                {
                    function f() { continue } //~ ERROR: keyword `continue` needs to be inside a for-loop body
                }
            }
        }
    }
}
