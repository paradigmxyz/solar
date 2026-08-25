// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

abstract contract MetadataStrings {
    function _document() internal pure returns (string memory) {
        return '{"name":"Solar Program Data","description":"shared program data keeps long metadata literals compact while preserving exact bytes across constructor and runtime code; offsets must survive final code layout and child deployment","image":"ipfs://bafybeigdyrzt5sfp7udm7hu76u3z7z4by5jx5f6x4x4h7c5m2yq3w6a7ba","external_url":"https://example.invalid/solar/data"}';
    }

    function _excerpt() internal pure returns (string memory) {
        return "shared program data keeps long metadata literals compact while preserving exact bytes across constructor and runtime code; offsets must survive final code layout and child deployment";
    }
}

contract MetadataRenderer is MetadataStrings {
    bytes32 public immutable expectedDocumentHash;
    address public immutable factory;

    constructor(bytes32 expectedDocumentHash_) {
        expectedDocumentHash = expectedDocumentHash_;
        factory = msg.sender;
    }

    function document() external pure returns (string memory) {
        return _document();
    }

    function excerpt() external pure returns (string memory) {
        return _excerpt();
    }
}

contract MetadataFactory is MetadataStrings {
    mapping(bytes32 => address) public rendererForSalt;

    function document() external pure returns (string memory) {
        return _document();
    }

    function excerpt() external pure returns (string memory) {
        return _excerpt();
    }

    function documentHash() external pure returns (bytes32) {
        return keccak256(bytes(_document()));
    }

    function excerptHash() external pure returns (bytes32) {
        return keccak256(bytes(_excerpt()));
    }

    function rendererCreationCodeHash() external pure returns (bytes32) {
        return keccak256(type(MetadataRenderer).creationCode);
    }

    function deploy(bytes32 salt) external returns (MetadataRenderer renderer) {
        bytes32 documentHash = keccak256(bytes(_document()));
        renderer = new MetadataRenderer{salt: salt}(documentHash);
        rendererForSalt[salt] = address(renderer);
    }
}

contract ProgramDataTest is MetadataStrings {
    bytes32 private constant EXPECTED_DOCUMENT_HASH =
        0x0d641c8f1416e28890b6b04b6652dad6a6b74239553fffc3fa562e04a3ce5865;
    bytes32 private constant EXPECTED_EXCERPT_HASH = 0x549ff1fc65d77c316f2a341a63bbd66b79956e0e248b64ad963455088ba33dfd;

    function testOverlappingMetadata() public {
        MetadataFactory factory = new MetadataFactory();

        assert(bytes(_document()).length == 357);
        assert(bytes(_excerpt()).length == 182);
        assert(keccak256(bytes(_document())) == EXPECTED_DOCUMENT_HASH);
        assert(keccak256(bytes(_excerpt())) == EXPECTED_EXCERPT_HASH);
        assert(factory.documentHash() == EXPECTED_DOCUMENT_HASH);
        assert(factory.excerptHash() == EXPECTED_EXCERPT_HASH);
        assert(bytes(factory.document()).length == 357);
        assert(bytes(factory.excerpt()).length == 182);
        assert(keccak256(bytes(factory.document())) == EXPECTED_DOCUMENT_HASH);
        assert(keccak256(bytes(factory.excerpt())) == EXPECTED_EXCERPT_HASH);
    }

    function testOverlappingMetadataAcrossFactoryAndChild() public {
        MetadataFactory factory = new MetadataFactory();

        bytes32 firstSalt = keccak256("first renderer");
        bytes32 secondSalt = keccak256("second renderer");
        MetadataRenderer first = factory.deploy(firstSalt);
        MetadataRenderer second = factory.deploy(secondSalt);

        assert(address(first) != address(second));
        assert(factory.rendererForSalt(firstSalt) == address(first));
        assert(factory.rendererForSalt(secondSalt) == address(second));
        assert(first.factory() == address(factory));
        assert(second.factory() == address(factory));
        assert(first.expectedDocumentHash() == EXPECTED_DOCUMENT_HASH);
        assert(second.expectedDocumentHash() == EXPECTED_DOCUMENT_HASH);
        assert(keccak256(bytes(first.document())) == EXPECTED_DOCUMENT_HASH);
        assert(keccak256(bytes(second.document())) == EXPECTED_DOCUMENT_HASH);
        assert(keccak256(bytes(first.excerpt())) == EXPECTED_EXCERPT_HASH);
        assert(keccak256(bytes(second.excerpt())) == EXPECTED_EXCERPT_HASH);
        assert(factory.rendererCreationCodeHash() == keccak256(type(MetadataRenderer).creationCode));
    }
}
