# README, "Events on disk": Seconds by model. Verbatim from the README;
# tests/unit/mcp-reports.bats holds the two to each other.
map(select(.kind == "messages" and .error == null)) | group_by(.model) | map({model: .[0].model,
  requests: length, mean_seconds: (map(.seconds) | add / length), max_seconds: (map(.seconds) | max)})
