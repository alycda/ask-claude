# README, "Events on disk": Tokens per chat (sessions take their last total). Verbatim from the README;
# tests/unit/mcp-reports.bats holds the two to each other.
group_by(.chat) | map(.[0].usage_scope as $scope
  | def total(f): map(f // 0) | if $scope == "session" then max else add end;
  {chat: .[0].chat, kind: .[0].kind,
    input_tokens: total(.input_tokens), output_tokens: total(.output_tokens),
    cache_read: total(.cache_read), cache_write: total(.cache_write)})
