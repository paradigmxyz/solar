//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: SharedRevert::check 18446744073709551615, 18446744073709551615 => 1
//@ run-call-fail: SharedRevert::check 0, 18446744073709551615 => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000194d696c6573746f6e6520646561646c696e652070617373656400000000000000
//@ run-call-fail: SharedRevert::check 18446744073709551615, 0 => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000194d696c6573746f6e6520646561646c696e652070617373656400000000000000
//@ run-call: andSkipsRhs => 0, false
//@ run-call: orSkipsRhs => 0, true
//@ run-call: andRunsRhs => 1, true
//@ run-call: orRunsRhs => 1, true

contract LogicalShortCircuit {
    uint256 calls;

    function bump() external returns (bool) {
        calls++;
        return true;
    }

    function andSkipsRhs() external returns (uint256, bool) {
        bool result = false && this.bump();
        return (calls, result);
    }

    function orSkipsRhs() external returns (uint256, bool) {
        bool result = true || this.bump();
        return (calls, result);
    }

    function andRunsRhs() external returns (uint256, bool) {
        bool result = true && this.bump();
        return (calls, result);
    }

    function orRunsRhs() external returns (uint256, bool) {
        bool result = false || this.bump();
        return (calls, result);
    }
}

contract SharedRevert {
    enum AgreementStatus { Created, Funded, InProgress, Completed, Refunded, Disputed }

    struct Agreement {
        uint256 id;
        address payable shipper;
        address payable carrier;
        string cargoDescription;
        uint256 totalPayloadValue;
        uint256 remainingLockedFunds;
        uint256 strictDeadline;
        uint256 currentMilestoneIndex;
        AgreementStatus status;
        uint256 milestoneCount;
        uint256 createdAt;
    }

    struct Milestone {
        string description;
        uint256 payoutPercentage;
        uint256 targetDeadline;
        bool isCompleted;
        bool isPaidOut;
        uint256 paidAmount;
    }

    mapping(uint256 => Agreement) public agreements;
    mapping(uint256 => Milestone[]) public agreementMilestones;
    mapping(uint256 => mapping(uint256 => bytes32)) public milestoneEvidence;
    mapping(uint256 => mapping(uint256 => uint256)) public evidenceSubmittedAt;
    bool private entered;

    modifier nonReentrant() {
        require(!entered, "Reentrant call");
        entered = true;
        _;
        entered = false;
    }

    modifier onlyCarrier(uint256 id) {
        require(msg.sender == agreements[id].carrier, "Not carrier");
        _;
    }

    event MilestoneEvidenceSubmitted(uint256 id, uint256 index, bytes32 hash, address sender);

    function submitMilestoneEvidence(uint256 _agreementId, uint256 _milestoneIndex, bytes32 _evidenceHash)
        external nonReentrant onlyCarrier(_agreementId)
    {
        Agreement storage ag = agreements[_agreementId];
        require(ag.status == AgreementStatus.Funded || ag.status == AgreementStatus.InProgress, "Agreement not active");
        require(_milestoneIndex == ag.currentMilestoneIndex && _milestoneIndex < ag.milestoneCount, "Invalid milestone index");
        require(
            block.timestamp <= ag.strictDeadline
                && block.timestamp <= agreementMilestones[_agreementId][_milestoneIndex].targetDeadline,
            "Milestone deadline passed"
        );
        require(_evidenceHash != bytes32(0), "Evidence hash required");
        require(milestoneEvidence[_agreementId][_milestoneIndex] == bytes32(0), "Evidence already submitted");
        milestoneEvidence[_agreementId][_milestoneIndex] = _evidenceHash;
        evidenceSubmittedAt[_agreementId][_milestoneIndex] = block.timestamp;
        emit MilestoneEvidenceSubmitted(_agreementId, _milestoneIndex, _evidenceHash, msg.sender);
    }

    function check(uint256 firstDeadline, uint256 secondDeadline) external returns (uint256) {
        Agreement storage ag = agreements[1];
        ag.carrier = payable(address(this));
        ag.status = AgreementStatus.Funded;
        ag.milestoneCount = 1;
        ag.strictDeadline = firstDeadline;
        agreementMilestones[1].push();
        agreementMilestones[1][0].targetDeadline = secondDeadline;
        this.submitMilestoneEvidence(1, 0, bytes32(uint256(1)));
        return uint256(milestoneEvidence[1][0]);
    }
}
