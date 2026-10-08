// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/reverting_call_virtual.sol
abstract contract B
{
        function iWillRevert() pure public virtual { revert(); }

        function test2(bool _param) pure external returns(uint256) //~ WARN: unnamed return variable can remain unassigned when the function is called when `C` is the most derived contract
        {
                if (_param) return 1;

                iWillRevert();
        }
}

contract C is B
{
        function iWillRevert() pure public override {  }

        function test(bool _param) pure external returns(uint256) //~ WARN: unnamed return variable can remain unassigned
        {
                if (_param) return 1;

                iWillRevert();
        }
}
