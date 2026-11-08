# README, "Events on disk": Advisor spend by advisor model. Verbatim from the README;
# tests/unit/mcp-reports.bats holds the two to each other.
map(select(.advisor != null)) | group_by(.advisor.model) | map({advisor: .[0].advisor.model,
  calls: (map(.advisor.calls) | add), input_tokens: (map(.advisor.input_tokens) | add),
  output_tokens: (map(.advisor.output_tokens) | add)})
