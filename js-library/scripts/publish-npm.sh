#!/usr/bin/env bash

# Reference: NPM RRFC --if-needed https://github.com/npm/rfcs/issues/466

LOCAL_SHASUM=$(npm pack --json | jq '.[] | .shasum' | sed -r 's/^"|"$//g')

NPM_TARBALL=$(npm show @dfinity/verifiable-credentials dist.tarball)
NPM_SHASUM=$(curl -s "$NPM_TARBALL" 2>&1 | shasum | cut -f1 -d' ')

if [ "$LOCAL_SHASUM" == "$NPM_SHASUM" ]; then
  echo "No changes in @dfinity/verifiable-credentials need to be published to NPM."
else
  npm publish --provenance --access public
fi
