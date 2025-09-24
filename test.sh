#!/bin/sh
#
# curl -v -o tmp.log 'https://github.com/ChaosNyaruko/ondict.git/info/refs?service=git-upload-pack'

# echo -en '0032want 42ef8cfdd14525539c47310fa2d83bcfe73b7ee4\n0000' | curl -v -H 'Content-Type: application/x-git-upload-pack-request' -H "Accept: application/x-git-upload-pack-result"   --data-binary @- 'https://github.com/ChaosNyaruko/ondict.git/git-upload-pack'
curl -v -o server.log -H 'Content-Type: application/x-git-upload-pack-request' --data-binary $'0032want 42ef8cfdd14525539c47310fa2d83bcfe73b7ee4\n00000009done\n' 'https://github.com/ChaosNyaruko/ondict.git/git-upload-pack'
