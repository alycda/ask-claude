# lib/reassemble.jq: fold a Messages API event stream into one message.
#
# Invocation (see tests/unit/reassemble.bats):
#     sed -n 's/^data: //p' chat.sse | jq -s -f lib/reassemble.jq
#
# -s (slurp) means the whole input is ONE value: an array of event objects,
# in stream order. This file is one expression evaluated against that array.
# The tests compare the output byte-for-byte with tests/fixtures/sse/*.message.json,
# so key order matters: jq keeps insertion order, and the seed object below
# fixes it.
#
# Shape of the fold:
#
#     reduce .[] as $e (SEED; UPDATE)
#
# `reduce` walks the array; `$e` is the current event; `.` inside UPDATE is
# the accumulator so far; UPDATE must produce the next accumulator. An
# `if/elif/else` chain on $e.type picks the branch; `else .` leaves the
# accumulator unchanged for events we ignore (ping).
#
# Blocks are collected in an object keyed by their index-as-string, because
# jq object keys must be strings and deltas address blocks by index. The last
# step turns that object back into the ordered `content` array.
#
# Useful operators you will meet:
#   .a += $x          append (string or number) to a path, creating it if absent
#   .a = $x           set
#   $x // $y          $y when $x is null or false
#   has("k")          true when the object has key k
#   fromjson          parse a JSON string into a value
#   del(.k)           drop a key
#   to_entries / sort_by / map   object -> [{key,value}] -> ordered -> values

reduce .[] as $e (
    # SEED: the empty message. Keys in the order the goldens expect.
    { blocks: {}, model: null, stop_reason: null, stop_details: null,
      usage: {}, container: null, error: null };

    # UPDATE
    if $e.type == "message_start" then
        .model=$e.message.model | .usage=$e.message.usage | .container=$e.message.container
    elif $e.type == "content_block_start" then
        .blocks[($e.index|tostring)] = ($e.content_block
          | if .type=="tool_use" or .type=="server_tool_use" then .partial="" else . end)
    elif $e.type == "content_block_delta" then
        ($e.index|tostring) as $i
          | if $e.delta.type=="text_delta" then .blocks[$i].text += $e.delta.text
            elif $e.delta.type=="thinking_delta" then .blocks[$i].thinking += $e.delta.thinking
            elif $e.delta.type=="signature_delta" then .blocks[$i].signature = $e.delta.signature
            elif $e.delta.type=="input_json_delta" then .blocks[$i].partial += $e.delta.partial_json
            else . end
    elif $e.type == "content_block_stop" then
        ($e.index|tostring) as $i
          | if (.blocks[$i] | has("partial")) then
                .blocks[$i].input = (if .blocks[$i].partial=="" then {} else (.blocks[$i].partial|fromjson) end)
                | del(.blocks[$i].partial)
            else . end
    elif $e.type == "message_delta" then
        # TODO: .stop_reason, .stop_details from $e.delta;
        #       .usage = (.usage + ($e.usage // {}));
        #       .container from $e.delta.container when present
        .stop_reason=$e.delta.stop_reason | .stop_details=$e.delta.stop_details
          | .usage=(.usage + ($e.usage // {}))
          | (if ($e.delta.container? // null) != null then .container=$e.delta.container else . end)
    elif $e.type == "error" then
        # TODO: .error = $e.error
        .
    else
        .
    end
)
# FINISH: .blocks (object keyed "0","1",...) -> .content (array in index order),
# then drop .blocks. The oracle does:
#   .content = (.blocks | to_entries | sort_by(.key|tonumber) | map(.value)) | del(.blocks)
| .
