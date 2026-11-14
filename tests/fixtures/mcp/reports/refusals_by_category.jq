# README, "Events on disk": Refusals by category. Verbatim from the README;
# tests/unit/mcp-reports.bats holds the two to each other.
map(select(.stop_reason == "refusal")) | group_by(.stop_category)
  | map({category: .[0].stop_category, refusals: length})
