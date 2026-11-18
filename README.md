# c3: the negatives on solar 65d87ab3, first box

Receipts only. Driver solar 65d87ab35420acd048ee1015113b6a887ab065ba (build provenance version 2), harness
ec92fe4086edaded1086879cd221061a4759d7ea, solidity-lean 334d1d924f8f6c29ac827fd81d1cc739721b479a. Cases: the 15 non-control rows
of cases.tsv (c3.tsv, sha256 f4126952...). Controls joined from the c1b archives (receipts e5b1ce91) and their c2 replays
(receipts 1df49bea), each checked against its digest list (JOIN PASS in join.log).

One m4-metal-small box, 3 cases at a time: six cases ran, nine were not admitted (no verdict, OPEN) and go to later parts.

- outbox/NAME.tgz with NAME.tgz.sha256 (digest taken where it ran, verified on copy).
- run-meta.tgz: run.log, table.md (with the control gate), producer.json/.log, join.log, the joined controls'
  join.json and gate.json, admission.txt, run-manifest.txt (full command), versions.txt, setup.log, run-scope.txt, go.log.
- receipts.sha256: every file here.
