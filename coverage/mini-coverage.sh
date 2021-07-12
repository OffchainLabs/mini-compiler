#!/usr/bin/env bash

LINES=$(cat $1 | awk '{$3="DA:"$3","$1; $1=""; sub(/\+/, 1); sub(/\-/, 0); sub(/^ /, ""); sub(/^std::/, "stdlib/"); sub(/^core::/, "builtin/"); print $0}')

CURR_FILE=""
CURR_COUNT=0

while read -r line; do

    FILE=${line%% *}

    if [[ "$FILE" != "$CURR_FILE" ]]; then
        if [[ "$CURR_FILE" != "" ]]; then
            echo "LF:$CURR_COUNT"
            echo "LH:0"
            echo "end_of_record"
        fi
        CURR_FILE=$FILE
        CURR_COUNT=0
        echo "SF:$FILE.mini"
    fi

    ((CURR_COUNT += 1))

    echo $line | cut -d" " -f2-

done <<< "$LINES"

echo "LF:$CURR_COUNT"
echo "LH:0"
echo "end_of_record"
