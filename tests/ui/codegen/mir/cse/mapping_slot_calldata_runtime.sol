//@ codegen-matrix: standard
//@ run-call: register "name"
//@ run-call: register ""
//@ run-call: registerTwice "name" => 0x3a81d6fc
//@ run-call: registerAndUpdate "name", 0x0000000000000000000000000000000000001234 => 0x0000000000000000000000000000000000001234
//@ run-call: registerAndUpdate "very-long-subdomain-name-with-more-bytes", 0x0000000000000000000000000000000000001234 => 0x0000000000000000000000000000000000001234
//@ run-call: lookup "missing" => 0x0000000000000000000000000000000000000000
//@ run-call-fail: update "name", 0x0000000000000000000000000000000000001234 => 0x82b42900
contract LilENS {
    error Unauthorized();
    error AlreadyRegistered();

    mapping(string => address) public lookup;

    function register(string calldata name) public payable {
        if (lookup[name] != address(0)) revert AlreadyRegistered();
        lookup[name] = msg.sender;
    }

    function update(string calldata name, address addr) public payable {
        if (msg.sender != lookup[name]) revert Unauthorized();
        lookup[name] = addr;
    }

    function registerTwice(string calldata name) external returns (bytes4) {
        register(name);
        try this.register(name) {} catch (bytes memory reason) {
            return bytes4(reason);
        }
        return 0;
    }

    function registerAndUpdate(string calldata name, address addr) external returns (address) {
        register(name);
        update(name, addr);
        return lookup[name];
    }
}
