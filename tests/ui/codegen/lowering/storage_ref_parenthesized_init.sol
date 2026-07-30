//@compile-flags: -Zcodegen --emit=bin-runtime
// ported-from: test/foundry/new/helpers/CriteriaResolverHelper.sol

// A parenthesized storage-reference initializer. Parentheses reach HIR as a
// one-element tuple, which the slot resolver did not look through, so
// `T storage r = (m[k]);` was rejected even though the unparenthesized form
// works. Parentheses do not change what is addressed, so resolve the inner
// expression's slot.
contract StorageRefParenthesizedInit {
    struct WildcardIdentifier {
        bool set;
        uint256 value;
    }

    mapping(bytes32 => WildcardIdentifier) private _wildcardIdentifierForGivenItemHash;

    function put(bytes32 itemHash, uint256 value) external returns (uint256) {
        WildcardIdentifier storage id = (
            _wildcardIdentifierForGivenItemHash[itemHash]
        );
        id.set = true;
        id.value = value;
        return _wildcardIdentifierForGivenItemHash[itemHash].value;
    }
}
