#!/bin/bash

DB="diary_app_cache"

TABLES="
diary_entries
"

for T in $TABLES;
do
    psql $DB -c "DELETE FROM $T";
done
