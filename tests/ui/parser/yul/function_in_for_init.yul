// ported-from: test/libyul/yulSyntaxTests/function_defined_in_init_block_1.yul
// ported-from: test/libyul/yulSyntaxTests/function_defined_in_init_block_2.yul
// ported-from: test/libyul/yulSyntaxTests/function_defined_in_init_block_3.yul
// ported-from: test/libyul/yulSyntaxTests/function_defined_in_init_nested_1.yul
// ported-from: test/libyul/yulSyntaxTests/function_defined_in_init_nested_2.yul
// ported-from: test/libyul/yulSyntaxTests/function_defined_in_init_nested_3.yul
{
    {
        for { } 1 { function f() {} } {}
    }
    {
        for { } 1 {} { function f() {} }
    }
    {
        for { function f() {} } 1 {} {} //~ ERROR: functions cannot be defined inside a for-loop init block
    }
    {
        for {
            for {} 1 { function f() {} }
            {}
        } 1 {}
        {}
    }
    {
        for { for {function foo() {}} 1 {} {} } 1 {} {} //~ ERROR: functions cannot be defined inside a for-loop init block
    }
    {
        for {}
            1
            {for {function foo() {}} 1 {} {} } //~ ERROR: functions cannot be defined inside a for-loop init block
        {}
    }
}
