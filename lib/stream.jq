# lib/stream.jq: print a Messages API event stream while it arrives.
#
# Invocation (see tests/unit/stream.bats):
#     sed -n 's/^data: //p' chat.sse | jq --unbuffered -rj -f lib/stream.jq
#
# Unlike lib/reassemble.jq this is NOT slurped: jq evaluates this file once
# per event, as each line arrives, so the reply appears token by token.
#   -r            print strings raw, without quotes
#   -j            join outputs with no newline between them
#   --unbuffered  flush after every output
# So every event must produce exactly one string, and "" means "print
# nothing". The tests compare the bytes with tests/fixtures/sse/*.stream.txt.
#
# Shape:
#
#     if .type == A then STRING elif ... else "" end
#
# Useful operators you will meet:
#   "\(expr)"          string interpolation
#   $x // "?"          "?" when $x is null or missing
#   a and b            both conditions

if .type == "content_block_start" then
    (if .content_block.type=="thinking" then "(thinking) "
             elif .content_block.type=="server_tool_use" then "(running code) "
             elif .content_block.type=="bash_code_execution_tool_result" then
                 "(result rc=\(.content_block.content.return_code // "?")) "
             else "" end)
elif .type == "content_block_delta" and .delta.type == "text_delta" then
    # TODO: the text itself. Other deltas (thinking, signature, tool input)
    #       fall through to the else branch and print nothing.
    ""
elif .type == "message_stop" then
    # TODO: the final newline
    ""
else
    ""
end
