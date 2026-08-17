def nonempty_string:
  if type == "string" then length > 0 else false end;

def sha256:
  if type == "string" then test("^[0-9a-f]{64}$") else false end;

def git_revision:
  if type == "string" then test("^[0-9a-f]{40}$") else false end;

def observed_version_matches($observed; $locked):
  if (($observed | type) == "string" and ($locked | type) == "string") then
    any(
      $observed | scan("[A-Za-z0-9.-]+");
      . == $locked
      or . == ("v" + $locked)
      or startswith($locked + "-")
    )
  else
    false
  end;

def pinned_digest_matches($expected; $actual):
  ($expected | sha256) and ($actual | sha256) and $expected == $actual;

def server_evidence:
  (.version | nonempty_string)
  and (.locked_version | nonempty_string)
  and observed_version_matches(.version; .locked_version)
  and (.source | type == "object")
  and (.source.url | nonempty_string)
  and (.source.revision | git_revision)
  and (.executable_sha256 | sha256)
  and (.artifact_sha256 | sha256)
  and (if .artifact_expected_sha256 == null then
    .artifact_sha256 == .executable_sha256
  else
    pinned_digest_matches(.artifact_expected_sha256; .artifact_sha256)
  end);

def native_compiler_evidence($compiler; $actual_digest; $observed_version):
  ($compiler | type == "object")
  and ($compiler.version | nonempty_string)
  and ($compiler.native | nonempty_string)
  and pinned_digest_matches($compiler.native_sha256; $actual_digest)
  and ($observed_version | nonempty_string)
  and observed_version_matches($observed_version; $compiler.version);

def soljson_compiler_evidence($compiler; $actual_digest):
  ($compiler.soljson | nonempty_string)
  and pinned_digest_matches($compiler.soljson_sha256; $actual_digest);

(.workloads | map(.id)) as $workload_ids
| (.workloads
    | map({ key: .id, value: { fixture: .fixture, repetitions: .repetitions } })
    | from_entries) as $workloads
| (.servers | map(.id)) as $servers
| (.fixtures | map(.id)) as $fixtures
| .fixtures as $fixture_metadata
| (.summaries | map("\(.server)\u0000\(.workload)")) as $summary_keys
| ([$servers[] as $server | $workloads | keys[] as $workload
    | "\($server)\u0000\($workload)"]) as $expected_keys
| .schema_version == 5
and .config_schema_version == 1
and (.config_sha256 | sha256)
and (.servers_lock_sha256 | sha256)
and (.fixtures_lock_sha256 | sha256)
and (.harness_git_revision | git_revision)
and .harness_git_dirty == false
and .profile == "publish"
and .repeat_override == null
and .environment.authoritative == true
and .environment.network_isolated == true
and ($workload_ids | length > 0 and length == (unique | length))
and ($servers | length > 0 and length == (unique | length))
and ($fixtures | length > 0 and length == (unique | length))
and ($workloads | length == ($workload_ids | length))
and ($summary_keys | sort == ($expected_keys | sort))
and all(.servers[]; .status == "available" and server_evidence)
and all($workloads[];
  .fixture as $fixture
  | ([$fixture_metadata[] | select(.id == $fixture)]) as $matches
  | ($matches | length) == 1
  and ($matches[0] as $metadata
    | ($metadata.content_sha256 | sha256)
    and ($metadata.source_file_count > 0)
    and (if $metadata.source == null then true else
      ($metadata.source | type == "object")
      and ($metadata.source.url | nonempty_string)
      and ($metadata.source.revision | git_revision)
      and ($metadata.revision | git_revision)
      and $metadata.source.revision == $metadata.revision
    end)
    and native_compiler_evidence(
      $metadata.solc;
      $metadata.solc_native_sha256;
      $metadata.solc_native_version
    )
    and soljson_compiler_evidence($metadata.solc; $metadata.solc_soljson_sha256)
    and native_compiler_evidence(
      $metadata.foundry;
      $metadata.foundry_native_sha256;
      $metadata.foundry_native_version
    )
    and ($metadata.foundry.archive_sha256 | sha256)))
and all(.summaries[];
  .fixture == $workloads[.workload].fixture
  and .status == "pass"
  and .successful_runs == $workloads[.workload].repetitions
  and (.status_counts | keys) == ["pass"]
  and .status_counts.pass == .successful_runs)
