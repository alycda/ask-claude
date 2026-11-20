# Review policy

@CODING_STANDARDS.md

This file holds how code review behaves in this repository: what a finding
needs, what the severities mean here, and what not to flag.

## What a finding needs

- A reproduction the reviewer ran, or a trace from an entry point to a
  sink with the controlling principal named. The security audit's records
  (`docs/security-audit/`) are the shape: root cause, invariant, trace,
  evidence, bounded reproduction, remediation with a regression case.
- The regression test it proposes must fail on the parent commit. "Fails
  on the previous commit" is a claim every fix commit here makes and
  checks.
- A lead without a second principal, such as one that needs another local
  account or a deployment fact outside the source, is recorded under needs
  validation with an owner-observed check, not reported as a finding.

## Severities

- **high / critical**: another principal (an endpoint, another local
  account, a commit author in a checked range) gets code execution, a
  credential, or a write outside the state directory. The audit found one
  medium and seven low; nothing here has been high yet.
- **medium**: the same principal gets a write or a grant inside the state
  directory, or a displayed value that differs from the one approved.
- **low**: hardening with the same trust as the script, or a defect
  reachable only by the user's own input.

## What not to flag

- Behaviour that matches a golden is not a defect: the golden is what the
  oracle does today. Flag it as a candidate for a golden change, which is
  its own commit saying why the oracle was wrong.
- Warnings in a reference or imported script that the port has already
  fixed. The shellcheck findings in the imported `claude.sh` were left
  there on purpose while it was the oracle.
- A test commit that is red. It is meant to be red on its parent for
  exactly the tests it adds.
