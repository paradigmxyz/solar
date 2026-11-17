# Adversarial negatives on solar afaefd40

Receipts only. Driver: solar afaefd409e1bab75ab0453eaba93fc06f197dd44. Harness:
5e14063a246e7d0f79b60c0fd359128044861e7b (tools/certify-adversarial). solidity-lean:
334d1d924f8f6c29ac827fd81d1cc739721b479a. One run, one machine (12 cores, 46 GiB, no swap), in a
systemd scope whose memory.max read back as 45097156608 with memory.swap.max 0 (admission.txt).

- outbox/NAME.tgz: each case's directory without its work tree, with MANIFEST.sha256 inside;
  NAME.tgz.sha256 is the digest taken where it ran.
- table.md, run.log: the run's table and log. producer.json: the solar binary every case used.
- versions.txt: every tool version. setup.log: the machine's setup.
- harness-7bafa4cd6..5e14063a.diff: the harness changes past the cases' commit.
- SHA256SUMS: every file here.
