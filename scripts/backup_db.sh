#!/bin/bash

DB="diary_app_cache"
BUCKET="diary-db-backup"

TABLES="
diary_cache
diary_entries
"

mkdir -p backup

for T in $TABLES;
do
    psql $DB -c "COPY $T TO STDOUT" | gzip > backup/${T}.sql.gz
    aws s3 cp backup/${T}.sql.gz s3://${BUCKET}/${T}.sql.gz
done
