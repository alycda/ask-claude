# README, "Events on disk": Cache hit rate by model. Verbatim from the README;
# tests/unit/mcp-reports.bats holds the two to each other.
map(select(.kind == "messages" and .error == null)) | group_by(.model) | map({model: .[0].model,
  cache_hit: ((map(.cache_read) | add) / ([map(.input_tokens + .cache_read + .cache_write) | add, 1] | max))})
