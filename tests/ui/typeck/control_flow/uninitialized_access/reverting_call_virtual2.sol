// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/reverting_call_virtual2.sol
abstract contract B
{
        function iWillRevert() pure public virtual { }

        function test(bool _param) pure external returns(uint256) //~ WARN: unnamed return variable can remain unassigned
        {
                if (_param) return 1;

                iWillRevert();
        }
}

contract C is B
{
        function iWillRevert() pure public override { revert(); }
}
