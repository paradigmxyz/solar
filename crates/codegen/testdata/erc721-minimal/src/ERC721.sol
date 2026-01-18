// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ERC721 {
    string public name;
    string public symbol;
    
    mapping(uint256 => address) public ownerOf;
    mapping(address => uint256) public balanceOf;
    mapping(uint256 => address) public getApproved;
    mapping(address => mapping(address => bool)) public isApprovedForAll;
    
    uint256 private _nextTokenId;
    
    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);
    event Approval(address indexed owner, address indexed approved, uint256 indexed tokenId);
    event ApprovalForAll(address indexed owner, address indexed operator, bool approved);
    
    constructor(string memory _name, string memory _symbol) {
        name = _name;
        symbol = _symbol;
    }
    
    function mint(address to) external returns (uint256) {
        require(to != address(0), "mint to zero address");
        
        uint256 tokenId = _nextTokenId++;
        balanceOf[to]++;
        ownerOf[tokenId] = to;
        
        emit Transfer(address(0), to, tokenId);
        return tokenId;
    }
    
    function burn(uint256 tokenId) external {
        address owner = ownerOf[tokenId];
        require(owner != address(0), "token does not exist");
        require(msg.sender == owner || isApprovedForAll[owner][msg.sender] || getApproved[tokenId] == msg.sender, "not authorized");
        
        balanceOf[owner]--;
        delete ownerOf[tokenId];
        delete getApproved[tokenId];
        
        emit Transfer(owner, address(0), tokenId);
    }
    
    function transferFrom(address from, address to, uint256 tokenId) external {
        require(from == ownerOf[tokenId], "wrong owner");
        require(to != address(0), "transfer to zero address");
        require(
            msg.sender == from || 
            isApprovedForAll[from][msg.sender] || 
            getApproved[tokenId] == msg.sender,
            "not authorized"
        );
        
        balanceOf[from]--;
        balanceOf[to]++;
        ownerOf[tokenId] = to;
        
        delete getApproved[tokenId];
        
        emit Transfer(from, to, tokenId);
    }
    
    function approve(address to, uint256 tokenId) external {
        address owner = ownerOf[tokenId];
        require(msg.sender == owner || isApprovedForAll[owner][msg.sender], "not authorized");
        
        getApproved[tokenId] = to;
        emit Approval(owner, to, tokenId);
    }
    
    function setApprovalForAll(address operator, bool approved) external {
        isApprovedForAll[msg.sender][operator] = approved;
        emit ApprovalForAll(msg.sender, operator, approved);
    }
}
