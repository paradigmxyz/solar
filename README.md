# Adversarial negatives on solar 92c59b66 (freeze 4.1)

Receipts only. Driver: solar 92c59b665bbd7ce17b39dd2317e5e66379408528. Harness: 10b8ae821882b0c7bde73f8a175c2230c6ca5060
(stages.tsv 23ff1ff7, cases.tsv c34490ff). solidity-lean 334d1d924f8f6c29ac827fd81d1cc739721b479a with a package cache
fetched by lake at the same commit (--packages-from; the records' packages.cache_sha256
5df059807ee5f620fc453d17b47f4db8e2c4d690b77ca918195eabe38d69b8a2). One machine: 48 cores, 377 GiB, no swap; run scope
memory.max 10737418240, each case in its own scope with memory.max 98624364544 and swap 0.

Result: control gate FAIL. All three controls reached CERTIFIED (exit 0, tier certified, strict holds, replay
closure fully replayed with trusted 0), but each independent --replay was cut off by the case deadline (exit 124)
during its probe step, so no negative counts. Two negatives finished; 13 were not admitted (time).

- run-meta.tgz: run.log, table.md, producer.json/.log, admission.txt, run-manifest.txt, versions.txt, setup.log,
  run-scope.txt, and the last lines of each control's replay log.
- outbox/NAME.tgz with NAME.tgz.sha256 (digest taken where it ran) for the cases copied off.
- MISSING.txt: what did not leave the machine before access ended.
- receipts.sha256: every file here.
