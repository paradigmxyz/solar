//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=mir

contract MappingDynamicKey {
    mapping(string => address) public lookup;

    function set(string memory name, address owner) public {
        lookup[name] = owner;
    }

    function get(string memory name) public view returns (address) {
        return lookup[name];
    }
}

// Every dynamic-key path must hash `key bytes ++ 32-byte slot` per spec
// (applied per level for nested mappings).
contract MappingDynamicKeyPaths {
    mapping(string => uint256) public flat;
    mapping(string => mapping(address => uint256)) public nestedFirst;
    mapping(address => mapping(string => uint256)) public nestedSecond;
    string public skey;

    // Literal keys hash exactly the literal's bytes, hitting the same slot
    // as the equivalent runtime key.
    function setLit(uint256 v) public {
        flat["hello"] = v;
    }

    function setLitLong(uint256 v) public {
        flat["a literal key longer than thirty-two bytes, hashed in full"] = v;
    }

    // Nested mappings dispatch on the key type at every level.
    function setNestedFirst(string memory k, address a, uint256 v) public {
        nestedFirst[k][a] = v;
    }

    function getNestedFirst(string memory k, address a) public view returns (uint256) {
        return nestedFirst[k][a];
    }

    function setNestedSecond(address a, string memory k, uint256 v) public {
        nestedSecond[a][k] = v;
    }

    // Storage string key: materialized to memory, then hashed as bytes.
    function setSkey(string memory s) public {
        skey = s;
    }

    function setViaStorageKey(uint256 v) public {
        flat[skey] = v;
    }

    // Calldata keys are staged at the unbumped free-memory scratch; keys
    // longer than 32 bytes must not clobber the free memory pointer or the
    // allocation that follows.
    function setThenAlloc(string calldata k, uint256 v) public returns (uint256) {
        flat[k] = v;
        bytes memory out = new bytes(32);
        return out.length;
    }
}
