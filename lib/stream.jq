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
#
# Everything printed goes through `safe`: the reply is model- and
# server-controlled, and jq -r turns JSON escapes such as \u001b back into raw
# bytes. `safe` keeps tab and newline and drops every other C0/C1 control
# character (ESC, BEL, CR, DEL, CSI U+009B ...), so the reply cannot drive the
# terminal: no title or clipboard writes, no erased or forged lines.

def safe:
    if type == "string" then
        explode | map(select(. == 9 or . == 10 or (. >= 32 and . != 127 and (. < 128 or . > 159)))) | implode
    else . end;

(if .type == "content_block_start" then
    (if .content_block.type=="thinking" then "(thinking) "
             elif .content_block.type=="server_tool_use" and (.content_block.name == "web_search" or .content_block.name == "web_fetch") then "(\(.content_block.name)) "
             elif .content_block.type=="server_tool_use" then "(running code) "
             elif .content_block.type=="mcp_tool_use" then "(mcp \(.content_block.server_name // "?")) "
             elif .content_block.type=="bash_code_execution_tool_result" then
                 "(result rc=\(.content_block.content.return_code | if type == "number" then tostring else "?" end)) "
             else "" end)
elif .type == "content_block_delta" and .delta.type == "text_delta" then
    .delta.text
elif .type == "message_stop" then
    "\n"
else
    ""
end) | safe
