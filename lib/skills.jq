# lib/skills.jq: turn the -s list into the container's skills array.
#
# Invocation (see tests/unit/request.bats):
#     printf 'pptx,skill_abc@skver_1' | tr ',' '\n' | jq -R -s -c -f lib/skills.jq
#
#   -R  read raw text, not JSON
#   -s  slurp: the whole input is ONE string, "pptx\nskill_abc@skver_1"
#   -c  compact output, one line
#
# Each line is <skill_id>[@<version>]. Output, one object per non-empty line:
#     {skill_id, version, type}
#   version  the part after "@", or "latest" when there is none
#   type     "custom" for ids starting with "skill_" (your uploads),
#            "anthropic" for the rest (pptx, xlsx, docx, pdf)
# Empty input gives [].
#
# Useful operators you will meet:
#   split("\n")         string -> array of lines
#   select(length > 0)  keep non-empty strings
#   .[1] // "latest"    second element, or "latest" when missing
#   startswith("x")     true when the string begins with x

split("\n") | map(select(length>0) | split("@")
  | {skill_id:.[0], version:(.[1] // "latest"),
      type:(if (.[0]|startswith("skill_")) then "custom" else "anthropic" end)})
